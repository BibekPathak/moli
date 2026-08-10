"""Per-engine WPT case runner.

Connects to a launched :class:`EngineDriver`, navigates the engine to each
WPT case URL, and polls ``window.__bench_wpt__`` (installed by the fixture
server's testharnessreport.js bridge) for the testharness completion result.

Classifies each result as ``pass`` / ``fail`` / ``timeout`` / ``crash`` /
``error`` so that the cross-engine matrix can be built without the runner
making subjective judgments about WHY a case failed.
"""

from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass, field
from typing import Any

from ..raw_cdp import RawCdpClient, RawCdpError, connect_raw_cdp
from .engine import EngineDriver, EngineDriverHandle

CaseRun = tuple[str, str] | tuple[str, str, float]


# WPT testharness.js status constants (testharness.js: TestsStatus)
HARNESS_STATUS_OK = 0
HARNESS_STATUS_ERROR = 1
HARNESS_STATUS_TIMEOUT = 2
HARNESS_STATUS_PRECONDITION_FAILED = 3

HARNESS_STATUS_NAMES = {
    HARNESS_STATUS_OK: "OK",
    HARNESS_STATUS_ERROR: "ERROR",
    HARNESS_STATUS_TIMEOUT: "TIMEOUT",
    HARNESS_STATUS_PRECONDITION_FAILED: "PRECONDITION_FAILED",
}

# WPT testharness.js per-test status constants (testharness.js: Test.statuses)
TEST_STATUS_PASS = 0
TEST_STATUS_FAIL = 1
TEST_STATUS_TIMEOUT = 2
TEST_STATUS_NOTRUN = 3
TEST_STATUS_PRECONDITION_FAILED = 4

TEST_STATUS_NAMES = {
    TEST_STATUS_PASS: "PASS",
    TEST_STATUS_FAIL: "FAIL",
    TEST_STATUS_TIMEOUT: "TIMEOUT",
    TEST_STATUS_NOTRUN: "NOTRUN",
    TEST_STATUS_PRECONDITION_FAILED: "PRECONDITION_FAILED",
}

MAX_RECORDED_FAILURES = 40
MAX_FAILURE_MESSAGE_CHARS = 500
FINAL_PAYLOAD_SOURCES = {"completion-callback", "done-hook", "done-hook-late"}


@dataclass
class CaseResult:
    case_path: str
    url: str
    status: str  # "pass" | "fail" | "timeout" | "crash" | "error"
    duration_ms: float | None
    harness_status: int | None = None
    harness_message: str | None = None
    subtests_total: int = 0
    subtests_pass: int = 0
    subtests_fail: int = 0
    subtests_timeout: int = 0
    subtests_notrun: int = 0
    error: str | None = None
    console_errors: int = 0
    js_exceptions: int = 0
    payload_source: str | None = None
    failures: list[dict[str, Any]] = field(default_factory=list)
    failure_names: list[str] = field(default_factory=list)


@dataclass
class EngineRunResult:
    engine: str
    binary: str | None
    binary_sha256: str | None
    binary_version: str | None
    endpoint: str
    ready_ms: float | None
    cases: list[CaseResult] = field(default_factory=list)
    shutdown_info: dict[str, Any] = field(default_factory=dict)
    setup_error: str | None = None


_TRACE_METHODS = {"Runtime.consoleAPICalled", "Runtime.exceptionThrown", "Log.entryAdded"}


def _count_traces(messages: list[dict[str, Any]]) -> tuple[int, int]:
    console_errors = 0
    js_exceptions = 0
    for message in messages:
        method = message.get("method")
        if method == "Runtime.exceptionThrown":
            js_exceptions += 1
        elif method == "Runtime.consoleAPICalled":
            params = message.get("params") or {}
            if params.get("type") in {"error", "assert"}:
                console_errors += 1
        elif method == "Log.entryAdded":
            entry = (message.get("params") or {}).get("entry") or {}
            if entry.get("level") == "error":
                console_errors += 1
    return console_errors, js_exceptions


def _recorded_failures(tests: Any) -> list[dict[str, Any]]:
    if not isinstance(tests, list):
        return []
    failures: list[dict[str, Any]] = []
    for entry in tests:
        if not isinstance(entry, dict):
            continue
        status = entry.get("status")
        if status == TEST_STATUS_PASS:
            continue
        failure: dict[str, Any] = {
            "status": status,
            "status_name": TEST_STATUS_NAMES.get(status) if isinstance(status, int) else None,
        }
        name = entry.get("name")
        if isinstance(name, str):
            failure["name"] = name
        message = entry.get("message")
        if isinstance(message, str) and message:
            failure["message"] = message[:MAX_FAILURE_MESSAGE_CHARS]
            if len(message) > MAX_FAILURE_MESSAGE_CHARS:
                failure["message_truncated"] = True
        failures.append(failure)
        if len(failures) >= MAX_RECORDED_FAILURES:
            break
    return failures


def _failure_name(entry: dict[str, Any], index: int) -> str:
    name = entry.get("name")
    if isinstance(name, str) and name:
        return name
    status = entry.get("status_name") or entry.get("status") or "failure"
    return f"<unnamed {status} #{index + 1}>"


def _failure_names(tests: Any) -> list[str]:
    if not isinstance(tests, list):
        return []
    names: list[str] = []
    for index, entry in enumerate(tests):
        if not isinstance(entry, dict):
            names.append(f"<malformed failure #{index + 1}>")
            continue
        status = entry.get("status")
        if status == TEST_STATUS_PASS:
            continue
        names.append(_failure_name(entry, index))
    return names


async def _attach_page(client: RawCdpClient) -> str:
    """Create a fresh BrowserContext + Target and return its sessionId.

    Falls back to the default target if Target.createBrowserContext is not
    supported. Enables Runtime/Page/Log so we can collect harness results
    and console errors.
    """

    browser_context_id: str | None = None
    try:
        ctx_id = await client.send("Target.createBrowserContext")
        ctx_resp, _ = await client.recv_until_id(ctx_id, timeout=5)
        value = (ctx_resp.get("result") or {}).get("browserContextId")
        if isinstance(value, str) and value:
            browser_context_id = value
    except RawCdpError:
        browser_context_id = None
    except asyncio.TimeoutError:
        browser_context_id = None

    target_params: dict[str, Any] = {"url": "about:blank"}
    if browser_context_id:
        target_params["browserContextId"] = browser_context_id
    target_id = await client.send("Target.createTarget", target_params)
    target_resp, _ = await client.recv_until_id(target_id, timeout=10)
    target = (target_resp.get("result") or {}).get("targetId")
    if not isinstance(target, str) or not target:
        raise RuntimeError(f"missing targetId in createTarget response: {target_resp}")

    attach_id = await client.send("Target.attachToTarget", {"targetId": target, "flatten": True})
    attach_resp, _ = await client.recv_until_id(attach_id, timeout=5)
    session_id = (attach_resp.get("result") or {}).get("sessionId")
    if not isinstance(session_id, str) or not session_id:
        raise RuntimeError(f"missing sessionId in attachToTarget response: {attach_resp}")

    for method in ("Runtime.enable", "Page.enable"):
        cmd_id = await client.send(method, session_id=session_id)
        await client.recv_until_id(cmd_id, timeout=5)
    for method in ("Log.enable",):
        try:
            cmd_id = await client.send(method, session_id=session_id)
            await client.recv_until_id(cmd_id, timeout=3)
        except (RawCdpError, asyncio.TimeoutError):
            pass

    return session_id


_HARNESS_PROBE_EXPRESSION = """
(function() {
  if (typeof window === 'undefined') return null;
  if (typeof window.__bench_wpt__ === 'undefined') return null;
  return window.__bench_wpt__;
})()
"""

_BRIDGE_INSTALLED_EXPRESSION = """
(function() {
  var t = (typeof window !== 'undefined') ? window.__bench_wpt_trace__ : null;
  if (!Array.isArray(t)) return false;
  for (var i = 0; i < t.length; i++) {
    if (t[i] && t[i].installing === true) return true;
  }
  return false;
})()
"""


async def _bridge_installed(client: RawCdpClient, session_id: str) -> bool:
    try:
        eval_id = await client.send(
            "Runtime.evaluate",
            {"expression": _BRIDGE_INSTALLED_EXPRESSION, "returnByValue": True},
            session_id=session_id,
        )
        response, _ = await client.recv_until_id(eval_id, timeout=5)
    except (RawCdpError, asyncio.TimeoutError):
        return False
    return bool(((response.get("result") or {}).get("result") or {}).get("value"))


async def _run_one_case(
    *,
    client: RawCdpClient,
    session_id: str,
    case_path: str,
    url: str,
    timeout_seconds: float,
) -> CaseResult:
    started = time.perf_counter()
    seen_messages: list[dict[str, Any]] = []
    try:
        nav_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
        _, nav_seen = await client.recv_until_id(nav_id, timeout=timeout_seconds)
        seen_messages.extend(nav_seen)
    except (RawCdpError, asyncio.TimeoutError) as error:
        return CaseResult(
            case_path=case_path,
            url=url,
            status="error",
            duration_ms=(time.perf_counter() - started) * 1000.0,
            error=f"navigate failed: {error}",
        )

    deadline = time.perf_counter() + timeout_seconds
    payload: Any = None
    while time.perf_counter() < deadline:
        try:
            eval_id = await client.send(
                "Runtime.evaluate",
                {
                    "expression": _HARNESS_PROBE_EXPRESSION,
                    "returnByValue": True,
                    "awaitPromise": False,
                },
                session_id=session_id,
            )
            response, eval_seen = await client.recv_until_id(eval_id, timeout=5)
            seen_messages.extend(eval_seen)
        except (RawCdpError, asyncio.TimeoutError) as error:
            return CaseResult(
                case_path=case_path,
                url=url,
                status="error",
                duration_ms=(time.perf_counter() - started) * 1000.0,
                error=f"evaluate failed: {error}",
            )
        result = ((response.get("result") or {}).get("result") or {})
        value = result.get("value")
        if isinstance(value, dict):
            source = value.get("source") if isinstance(value.get("source"), str) else None
            payload = value
            if source in FINAL_PAYLOAD_SOURCES:
                break
        await asyncio.sleep(0.05)

    duration_ms = (time.perf_counter() - started) * 1000.0
    console_errors, js_exceptions = _count_traces(seen_messages)
    # Distinguish "bridge never installed" (engine couldn't load
    # testharness.js at all) from "bridge installed but testharness never
    # produced results" (engine-side completion bug). A non-final payload also
    # proves the bridge was installed, but it is not enough to pass the case.
    bridge_installed = payload is not None or await _bridge_installed(client, session_id)
    return classify_payload(
        payload=payload if isinstance(payload, dict) else None,
        case_path=case_path,
        url=url,
        duration_ms=duration_ms,
        bridge_installed=bridge_installed,
        console_errors=console_errors,
        js_exceptions=js_exceptions,
    )


def classify_payload(
    *,
    payload: dict | None,
    case_path: str,
    url: str,
    duration_ms: float | None,
    bridge_installed: bool,
    console_errors: int = 0,
    js_exceptions: int = 0,
    error: str | None = None,
) -> CaseResult:
    """Map a bridge payload (or its absence) to a CaseResult.

    Used by both the CDP runner and the CLI HTTP-callback runner so the
    pass/fail/timeout/harness-stalled classification stays uniform.
    """

    if payload is None or not isinstance(payload, dict):
        return CaseResult(
            case_path=case_path,
            url=url,
            status="harness-stalled" if bridge_installed else "timeout",
            duration_ms=duration_ms,
            console_errors=console_errors,
            js_exceptions=js_exceptions,
            error=error or (
                "bridge installed but testharness produced no result/completion callbacks"
                if bridge_installed
                else "testharness did not complete within timeout"
            ),
        )

    payload_source = payload.get("source")
    harness = payload.get("harness")
    tests = payload.get("tests")
    harness_status = harness.get("status") if isinstance(harness, dict) else None
    harness_message = harness.get("message") if isinstance(harness, dict) else None

    counts = {"pass": 0, "fail": 0, "timeout": 0, "notrun": 0, "other": 0}
    if isinstance(tests, list):
        for entry in tests:
            if not isinstance(entry, dict):
                counts["other"] += 1
                continue
            status = entry.get("status")
            if status == TEST_STATUS_PASS:
                counts["pass"] += 1
            elif status == TEST_STATUS_FAIL or status == TEST_STATUS_PRECONDITION_FAILED:
                counts["fail"] += 1
            elif status == TEST_STATUS_TIMEOUT:
                counts["timeout"] += 1
            elif status == TEST_STATUS_NOTRUN:
                counts["notrun"] += 1
            else:
                counts["other"] += 1

    total = sum(counts.values())
    overall = "pass"
    has_observed_failure = bool(counts["fail"] or counts["timeout"])
    if harness_status == HARNESS_STATUS_TIMEOUT:
        overall = "timeout"
        has_observed_failure = True
    elif harness_status == HARNESS_STATUS_ERROR:
        overall = "fail"
        has_observed_failure = True
    elif harness_status == HARNESS_STATUS_PRECONDITION_FAILED:
        overall = "fail"
        has_observed_failure = True
    elif counts["fail"] or counts["timeout"]:
        overall = "fail"
    elif total == 0:
        overall = "fail"

    if payload_source not in FINAL_PAYLOAD_SOURCES and not has_observed_failure:
        overall = "harness-stalled" if bridge_installed else "timeout"
        if error is None:
            source_label = (
                payload_source if isinstance(payload_source, str) else "non-final"
            )
            error = (
                f"testharness produced only {source_label} payload "
                "without final completion"
            )
    elif total == 0 and error is None:
        error = "testharness completed without reporting any subtests"
        if isinstance(harness_message, str) and harness_message:
            error = f"{error}: {harness_message}"

    return CaseResult(
        case_path=case_path,
        url=url,
        status=overall,
        duration_ms=duration_ms,
        harness_status=harness_status if isinstance(harness_status, int) else None,
        harness_message=harness_message if isinstance(harness_message, str) else None,
        subtests_total=total,
        subtests_pass=counts["pass"],
        subtests_fail=counts["fail"],
        subtests_timeout=counts["timeout"],
        subtests_notrun=counts["notrun"],
        console_errors=console_errors,
        js_exceptions=js_exceptions,
        payload_source=payload_source if isinstance(payload_source, str) else None,
        error=error,
        failures=_recorded_failures(tests),
        failure_names=_failure_names(tests),
    )


async def _run_async(
    *,
    driver: EngineDriver,
    binary_override: str | None,
    cases: list[CaseRun],
    case_timeout_seconds: float,
    launch_timeout_seconds: float,
) -> EngineRunResult:
    handle: EngineDriverHandle | None = None
    result = EngineRunResult(
        engine=driver.name,
        binary=None,
        binary_sha256=None,
        binary_version=None,
        endpoint="",
        ready_ms=None,
    )
    try:
        handle = driver.launch(binary_override=binary_override, ready_timeout_seconds=launch_timeout_seconds)
    except Exception as error:
        result.setup_error = f"launch failed: {error}"
        return result
    result.binary = str(handle.binary) if handle.binary else None
    result.binary_sha256 = handle.binary_sha256
    result.binary_version = handle.binary_version
    result.endpoint = handle.endpoint
    result.ready_ms = handle.ready_ms

    try:
        client = await connect_raw_cdp(handle.endpoint)
    except Exception as error:
        result.setup_error = f"cdp connect failed: {error}"
        result.shutdown_info = driver.shutdown(handle)
        return result

    try:
        session_id = await _attach_page(client)
    except Exception as error:
        result.setup_error = f"attach failed: {error}"
        try:
            await client.websocket.close()
        except Exception:
            pass
        result.shutdown_info = driver.shutdown(handle)
        return result

    relaunch_count = 0
    max_relaunches = 10
    consecutive_relaunch_failures = 0

    try:
        for case_index, case in enumerate(cases):
            case_path, url, timeout_seconds = _case_parts(case, case_timeout_seconds)
            # Engine died between cases?
            if handle.process.poll() is not None:
                exit_code = handle.process.returncode
                result.cases.append(
                    CaseResult(
                        case_path=case_path,
                        url=url,
                        status="crash",
                        duration_ms=None,
                        error=f"engine process exited with code {exit_code} (pre-case)",
                    )
                )
                relaunched = await _try_relaunch(
                    driver=driver,
                    binary_override=binary_override,
                    launch_timeout_seconds=launch_timeout_seconds,
                )
                if relaunched is None:
                    consecutive_relaunch_failures += 1
                    if consecutive_relaunch_failures >= 3:
                        for remaining in cases[case_index + 1:]:
                            rp, ru, _ = _case_parts(remaining, case_timeout_seconds)
                            result.cases.append(
                                CaseResult(
                                    case_path=rp, url=ru, status="crash",
                                    duration_ms=None,
                                    error="engine relaunch failed 3x; aborting",
                                )
                            )
                        break
                    continue
                consecutive_relaunch_failures = 0
                relaunch_count += 1
                # swap in new handle/client/session
                try:
                    await client.websocket.close()
                except Exception:
                    pass
                driver.shutdown(handle)  # ensure old handle fully reaped
                handle, client, session_id = relaunched
                continue

            try:
                case_result = await _run_one_case(
                    client=client,
                    session_id=session_id,
                    case_path=case_path,
                    url=url,
                    timeout_seconds=timeout_seconds,
                )
            except Exception as error:
                # CDP / websocket / asyncio explosion mid-case.
                proc_alive = handle.process.poll() is None
                case_result = CaseResult(
                    case_path=case_path,
                    url=url,
                    status="crash" if not proc_alive else "error",
                    duration_ms=None,
                    error=f"runner exception: {type(error).__name__}: {error}",
                )
                result.cases.append(case_result)
                if relaunch_count >= max_relaunches:
                    for remaining in cases[case_index + 1:]:
                        rp, ru, _ = _case_parts(remaining, case_timeout_seconds)
                        result.cases.append(
                            CaseResult(
                                case_path=rp, url=ru, status="crash",
                                duration_ms=None,
                                error=f"exceeded max relaunches ({max_relaunches})",
                            )
                        )
                    break
                # Tear down current connection + engine, relaunch fresh.
                try:
                    await client.websocket.close()
                except Exception:
                    pass
                driver.shutdown(handle)
                relaunched = await _try_relaunch(
                    driver=driver,
                    binary_override=binary_override,
                    launch_timeout_seconds=launch_timeout_seconds,
                )
                if relaunched is None:
                    consecutive_relaunch_failures += 1
                    if consecutive_relaunch_failures >= 3:
                        for remaining in cases[case_index + 1:]:
                            rp, ru, _ = _case_parts(remaining, case_timeout_seconds)
                            result.cases.append(
                                CaseResult(
                                    case_path=rp, url=ru, status="crash",
                                    duration_ms=None,
                                    error="engine relaunch failed 3x after crash; aborting",
                                )
                            )
                        break
                    # client/handle are dead; fabricate placeholders that will fail
                    # the poll() check next iteration -> retry relaunch path.
                    handle, client, session_id = await _wait_then_retry_launch(
                        driver,
                        binary_override,
                        launch_timeout_seconds,
                        result,
                        cases,
                        case_index,
                        case_timeout_seconds,
                    )
                    if handle is None:
                        break
                else:
                    consecutive_relaunch_failures = 0
                relaunch_count += 1
                if relaunched is not None:
                    handle, client, session_id = relaunched
                continue

            consecutive_relaunch_failures = 0
            result.cases.append(case_result)
    finally:
        try:
            await client.websocket.close()
        except Exception:
            pass
        try:
            result.shutdown_info = driver.shutdown(handle)
        except Exception:
            pass
    return result


async def _try_relaunch(
    *,
    driver: EngineDriver,
    binary_override: str | None,
    launch_timeout_seconds: float,
) -> tuple[EngineDriverHandle, RawCdpClient, str] | None:
    """Relaunch engine + reconnect CDP + reattach. Returns None on failure."""

    try:
        new_handle = driver.launch(
            binary_override=binary_override,
            ready_timeout_seconds=launch_timeout_seconds,
        )
    except Exception:
        return None
    try:
        new_client = await connect_raw_cdp(new_handle.endpoint)
    except Exception:
        try:
            driver.shutdown(new_handle)
        except Exception:
            pass
        return None
    try:
        new_session_id = await _attach_page(new_client)
    except Exception:
        try:
            await new_client.websocket.close()
        except Exception:
            pass
        try:
            driver.shutdown(new_handle)
        except Exception:
            pass
        return None
    return new_handle, new_client, new_session_id


async def _wait_then_retry_launch(
    driver: EngineDriver,
    binary_override: str | None,
    launch_timeout_seconds: float,
    result: EngineRunResult,
    cases: list[CaseRun],
    case_index: int,
    case_timeout_seconds: float,
) -> tuple[EngineDriverHandle | None, RawCdpClient | None, str | None]:
    await asyncio.sleep(1.0)
    relaunched = await _try_relaunch(
        driver=driver,
        binary_override=binary_override,
        launch_timeout_seconds=launch_timeout_seconds,
    )
    if relaunched is None:
        for remaining in cases[case_index + 1:]:
            rp, ru, _ = _case_parts(remaining, case_timeout_seconds)
            result.cases.append(
                CaseResult(
                    case_path=rp, url=ru, status="crash",
                    duration_ms=None,
                    error="engine relaunch failed after backoff; aborting",
                )
            )
        return None, None, None
    return relaunched


def _case_parts(case: CaseRun, default_timeout: float) -> tuple[str, str, float]:
    if len(case) == 3:
        return case[0], case[1], case[2]
    return case[0], case[1], default_timeout


def run_engine_on_cases(
    *,
    driver: EngineDriver,
    cases: list[CaseRun],
    binary_override: str | None = None,
    case_timeout_seconds: float = 30.0,
    launch_timeout_seconds: float = 30.0,
) -> EngineRunResult:
    """Synchronous wrapper around the async runner.

    ``cases`` is a list of ``(case_path, url)`` or
    ``(case_path, url, timeout_seconds)`` tuples where ``case_path`` is the
    WPT-relative path (used as case identity) and ``url`` is what the engine
    actually navigates to (loopback or external IPv6).
    """

    return asyncio.run(
        _run_async(
            driver=driver,
            binary_override=binary_override,
            cases=cases,
            case_timeout_seconds=case_timeout_seconds,
            launch_timeout_seconds=launch_timeout_seconds,
        )
    )


def case_result_to_dict(case: CaseResult) -> dict[str, Any]:
    return {
        "case_path": case.case_path,
        "url": case.url,
        "status": case.status,
        "duration_ms": case.duration_ms,
        "harness_status": case.harness_status,
        "harness_status_name": HARNESS_STATUS_NAMES.get(case.harness_status, None) if case.harness_status is not None else None,
        "harness_message": case.harness_message,
        "subtests": {
            "total": case.subtests_total,
            "pass": case.subtests_pass,
            "fail": case.subtests_fail,
            "timeout": case.subtests_timeout,
            "notrun": case.subtests_notrun,
        },
        "console_errors": case.console_errors,
        "js_exceptions": case.js_exceptions,
        "payload_source": case.payload_source,
        "error": case.error,
        "failures": case.failures,
        "failure_names": case.failure_names,
    }


def engine_result_to_dict(result: EngineRunResult) -> dict[str, Any]:
    return {
        "engine": result.engine,
        "binary": result.binary,
        "binary_sha256": result.binary_sha256,
        "binary_version": result.binary_version,
        "endpoint": result.endpoint,
        "ready_ms": result.ready_ms,
        "setup_error": result.setup_error,
        "shutdown": result.shutdown_info,
        "cases": [case_result_to_dict(c) for c in result.cases],
    }


def write_engine_result(path, result: EngineRunResult) -> None:
    """Write ``result`` to ``path`` as JSON. Convenience helper for CLI."""

    from pathlib import Path as _Path

    out = _Path(path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(engine_result_to_dict(result), indent=2, sort_keys=True), encoding="utf-8")
