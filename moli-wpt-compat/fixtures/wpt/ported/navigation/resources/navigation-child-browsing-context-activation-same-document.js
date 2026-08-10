(function () {
  const config = globalThis.__lmChildBrowsingContextActivationSameDocumentConfig;
  if (!config) {
    return;
  }

  function wait(ms) {
    return new Promise(function (resolve) {
      setTimeout(resolve, ms);
    });
  }

  function wait_for_value(read, description) {
    return Promise.race([
      new Promise(function (resolve, reject) {
        const deadline = Date.now() + 1000;
        function poll() {
          const value = read();
          if (value) {
            resolve(value);
            return;
          }
          if (Date.now() >= deadline) {
            reject(new Error("timed out waiting for " + description));
            return;
          }
          setTimeout(poll, 10);
        }
        poll();
      }),
      wait(1100).then(function () {
        throw new Error("timed out waiting for " + description);
      }),
    ]);
  }

  promise_test(async function () {
    const iframe = document.getElementById("child");
    const childUrl = new URL(iframe.getAttribute("src"), location.href).href;
    const childWindow = iframe.contentWindow;

    await wait_for_value(function () {
      return (
        childWindow.location.href === childUrl &&
        childWindow.navigation.currentEntry?.url === childUrl
      );
    }, "child initial navigation commit");

    const initial = snapshotChildActivation(iframe);

    childWindow.navigation.navigate("#one", {
      history: "push",
      state: { step: 1 },
    });
    await wait_for_value(function () {
      return childWindow.location.hash === "#one";
    }, "child same-document push");
    const afterPush = snapshotChildActivation(iframe);

    childWindow.history.back();
    await wait_for_value(function () {
      return childWindow.location.hash === "";
    }, "child history back");
    const afterBack = snapshotChildActivation(iframe);

    childWindow.history.forward();
    await wait_for_value(function () {
      return childWindow.location.hash === "#one";
    }, "child history forward");
    const afterForward = snapshotChildActivation(iframe);

    assert_equals(
      initial,
      childUrl + "||replace|" + childUrl + "|true",
      "initial child activation should point at the initial child document",
    );
    assert_equals(
      afterPush,
      childUrl + "||replace|" + childUrl + "#one|true",
      "same-document child push should keep activation pinned to the initial child document",
    );
    assert_equals(
      afterBack,
      childUrl + "||replace|" + childUrl + "|true",
      "child history.back() should keep activation pinned to the initial child document",
    );
    assert_equals(
      afterForward,
      childUrl + "||replace|" + childUrl + "#one|true",
      "child history.forward() should keep activation pinned to the initial child document",
    );
  }, config.testName);

  function snapshotChildActivation(iframe) {
    const childWindow = iframe.contentWindow;
    const activation = childWindow.navigation.activation;
    return [
      String(activation?.entry?.url ?? ""),
      String(activation?.from?.url ?? ""),
      String(activation?.navigationType ?? ""),
      String(childWindow.navigation.currentEntry?.url ?? ""),
      String(childWindow.navigation.transition === null),
    ].join("|");
  }
})();
