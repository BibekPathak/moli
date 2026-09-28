// Tests grouped by behavior. Shared fixtures live in the parent module.
use super::*;

#[tokio::test]
async fn worker_opfs_root_directory_and_file_handles_are_available() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const root = await navigator.storage.getDirectory();
            const directory = await root.getDirectoryHandle("worker-dir", { create: true });
            const file = await directory.getFileHandle("worker-file", { create: true });
            const writable = await file.createWritable();
            await writable.write("worker");
            await writable.close();
            const snapshot = await file.getFile();
            const entries = [];
            for await (const [name, child] of root) {
                entries.push(`${name}:${child.kind}`);
            }
            const resolved = await root.resolve(file);
            const concurrentIterator = directory.keys();
            const iteratorBatch = (await Promise.all([
                concurrentIterator.next(),
                concurrentIterator.next()
            ])).map(result => result.done ? "done" : result.value);
            await directory.removeEntry("worker-file");
            const removedFile = await directory.getFileHandle("worker-file").then(
                () => "present",
                error => error && error.name
            );
            await directory.remove();
            const removedDirectory = await root.getDirectoryHandle("worker-dir").then(
                () => "present",
                error => error && error.name
            );
            postMessage({
                root: [root.kind, root.name],
                brands: [
                    root instanceof FileSystemDirectoryHandle,
                    root instanceof FileSystemHandle,
                    file instanceof FileSystemFileHandle,
                    file instanceof FileSystemHandle,
                    snapshot instanceof File,
                    writable instanceof FileSystemWritableFileStream,
                    writable instanceof WritableStream
                ],
                file: [file.name, snapshot.name, snapshot.size],
                resolved,
                entries,
                iteratorBatch,
                removedFile,
                removedDirectory,
                syncAccessHandleConstructor: typeof FileSystemSyncAccessHandle
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"root":["directory",""],"brands":[true,true,true,true,true,true,true],"file":["worker-file","worker-file",6],"resolved":["worker-dir","worker-file"],"entries":["worker-dir:directory"],"iteratorBatch":["worker-file","done"],"removedFile":"NotFoundError","removedDirectory":"NotFoundError","syncAccessHandleConstructor":"function"}"#
    );
}

#[tokio::test]
async fn worker_opfs_handle_permissions_match_sandboxed_fixed_grants() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    return `resolved:${await promise}`;
                } catch (error) {
                    return `rejected:${error && error.name}`;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("permission", { create: true });
            postMessage({
                shape: [
                    typeof FileSystemHandle.prototype.queryPermission,
                    FileSystemHandle.prototype.queryPermission.length,
                    typeof FileSystemHandle.prototype.requestPermission,
                    FileSystemHandle.prototype.requestPermission.length
                ],
                rootRead: await root.queryPermission(),
                fileReadwrite: await file.queryPermission({ mode: "readwrite" }),
                requested: await file.requestPermission({ mode: "readwrite" }),
                invalid: await outcome(file.queryPermission({ mode: "invalid" })),
                illegal: await outcome(
                    FileSystemHandle.prototype.requestPermission.call({})
                )
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-permission-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"shape":["function",0,"function",0],"rootRead":"granted","fileReadwrite":"granted","requested":"granted","invalid":"rejected:TypeError","illegal":"rejected:TypeError"}"#
    );
}

#[tokio::test]
async fn worker_opfs_concurrent_file_moves_follow_storage_owner_order() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const source = await root.getDirectoryHandle("source", { create: true });
            const destination = await root.getDirectoryHandle("destination", { create: true });
            const file = await source.getFileHandle("before.txt", { create: true });
            const writer = await file.createWritable();
            await writer.write("worker ordered move");
            await writer.close();

            const firstMove = file.move(destination, "middle.txt");
            const nameWhilePending = file.name;
            const secondMove = file.move("after.txt");
            const snapshot = file.getFile();
            const resolved = root.resolve(file);
            const sameSelf = file.isSameEntry(file);
            const values = await Promise.all([
                firstMove,
                outcome(secondMove),
                snapshot,
                resolved,
                sameSelf
            ]);
            const retryAfterSettlement = await outcome(file.move("after.txt"));
            const sourceKeys = [];
            for await (const key of source.keys()) sourceKeys.push(key);
            const destinationKeys = [];
            for await (const key of destination.keys()) destinationKeys.push(key);
            postMessage({
                nameWhilePending,
                finalName: file.name,
                secondMove: values[1],
                retryAfterSettlement,
                text: await values[2].text(),
                resolved: values[3],
                sameSelf: values[4],
                sourceKeys,
                destinationKeys
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-move-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"nameWhilePending":"before.txt","finalName":"after.txt","secondMove":"NoModificationAllowedError","retryAfterSettlement":"resolved","text":"worker ordered move","resolved":["destination","middle.txt"],"sameSelf":true,"sourceKeys":[],"destinationKeys":["after.txt"]}"#
    );
}

#[tokio::test]
async fn worker_opfs_directory_move_uses_owner_order_and_subtree_locks() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const source = await root.getDirectoryHandle("source", { create: true });
            const destination = await root.getDirectoryHandle("destination", { create: true });
            const nested = await source.getDirectoryHandle("nested", { create: true });
            const file = await nested.getFileHandle("file.txt", { create: true });
            const writer = await file.createWritable();
            await writer.write("worker directory move");
            await writer.close();

            const lock = await file.createWritable({ keepExistingData: true });
            const subtreeLock = await outcome(source.move("blocked"));
            await lock.close();

            const firstMove = source.move(destination, "middle");
            const nameWhilePending = source.name;
            const secondMove = source.move("after");
            const movedNested = source.getDirectoryHandle("nested");
            const resolved = root.resolve(source);
            const [, secondMoveOutcome, nestedAfter, resolvedAfter] = await Promise.all([
                firstMove,
                outcome(secondMove),
                movedNested,
                resolved
            ]);
            const retryAfterSettlement = await outcome(source.move("after"));
            const staleNestedAfterRetry = await outcome(
                nestedAfter.getFileHandle("file.txt"));
            const finalNested = await source.getDirectoryHandle("nested");
            const movedFile = await finalNested.getFileHandle("file.txt");
            const sourceKeys = [];
            for await (const key of root.keys()) sourceKeys.push(key);
            const destinationKeys = [];
            for await (const key of destination.keys()) destinationKeys.push(key);
            postMessage({
                prototype: [
                    typeof FileSystemDirectoryHandle.prototype.move,
                    Object.prototype.hasOwnProperty.call(
                        FileSystemDirectoryHandle.prototype, "move")
                ],
                subtreeLock,
                nameWhilePending,
                finalName: source.name,
                secondMove: secondMoveOutcome,
                retryAfterSettlement,
                staleNestedAfterRetry,
                resolved: resolvedAfter,
                text: await (await movedFile.getFile()).text(),
                sourceKeys,
                destinationKeys
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-directory-move-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"prototype":["function",true],"subtreeLock":"NoModificationAllowedError","nameWhilePending":"source","finalName":"after","secondMove":"NoModificationAllowedError","retryAfterSettlement":"resolved","staleNestedAfterRetry":"NotFoundError","resolved":["destination","middle"],"text":"worker directory move","sourceKeys":["destination"],"destinationKeys":["after"]}"#
    );
}

#[tokio::test]
async fn worker_opfs_owner_state_materializes_on_first_operation() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const sameStorage = navigator.storage === navigator.storage;
            const root = await navigator.storage.getDirectory();
            postMessage({
                sameStorage,
                kind: root.kind,
                directoryConstructorInheritance:
                    Object.getPrototypeOf(FileSystemDirectoryHandle) ===
                        FileSystemHandle &&
                    Object.getPrototypeOf(
                        FileSystemDirectoryHandle.prototype
                    ) === FileSystemHandle.prototype,
                writableConstructorInheritance:
                    Object.getPrototypeOf(FileSystemWritableFileStream) ===
                        WritableStream &&
                    Object.getPrototypeOf(
                        FileSystemWritableFileStream.prototype
                    ) === WritableStream.prototype
            });
        });
        "#
        .into(),
        "https://opfs-owner-state-lazy-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"sameStorage":true,"kind":"directory","directoryConstructorInheritance":true,"writableConstructorInheritance":true}"#
    );
    let diagnostics = timeout(TIMEOUT, handle.resource_owner_slot_diagnostics())
        .await
        .expect("timed out waiting for worker OPFS diagnostics")
        .expect("worker OPFS diagnostics should complete");
    assert!(
        diagnostics.opfs_owner_state_materialized,
        "the first worker OPFS operation must allocate its owner state"
    );
    assert!(
        diagnostics.storage_constructor_materializations >= 2,
        "navigator.storage and the returned directory handle must materialize their constructors"
    );
    assert!(
        diagnostics.storage_manager_materialized,
        "reading navigator.storage must materialize one SameObject wrapper"
    );
    assert!(
        !diagnostics.storage_bucket_manager_materialized,
        "using navigator.storage must not materialize navigator.storageBuckets"
    );
}

#[tokio::test]
async fn worker_opfs_mutation_leases_span_owner_promise_settlement() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    const value = await promise;
                    return { status: "resolved", value };
                } catch (error) {
                    return { status: error && error.name };
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("mutation-lease.txt", { create: true });

            const movePromise = file.move("moved.txt");
            const writerDuringMove = outcome(
                file.createWritable({ mode: "exclusive" }));
            const [moveResult, moveConflict] = await Promise.all([
                outcome(movePromise),
                writerDuringMove
            ]);
            const writerAfterMove = await file.createWritable({ mode: "exclusive" });
            await writerAfterMove.close();

            const removePromise = file.remove();
            const syncDuringRemove = outcome(
                file.createSyncAccessHandle({ mode: "read-only" }));
            const [removeResult, removeConflict] = await Promise.all([
                outcome(removePromise),
                syncDuringRemove
            ]);
            const syncAfterRemove = await outcome(
                file.createSyncAccessHandle({ mode: "read-only" }));

            postMessage({
                moveResult: moveResult.status,
                moveConflict: moveConflict.status,
                removeResult: removeResult.status,
                removeConflict: removeConflict.status,
                syncAfterRemove: syncAfterRemove.status
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-mutation-lease-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"moveResult":"resolved","moveConflict":"NoModificationAllowedError","removeResult":"resolved","removeConflict":"NoModificationAllowedError","syncAfterRemove":"NotFoundError"}"#
    );
}

#[tokio::test]
async fn worker_teardown_drops_pending_opfs_mutation_completion_and_releases_lock() {
    ensure_v8();
    let bucket_store = crate::new_shared_storage_bucket_store();
    let storage_service = bucket_store.lock().storage_service();
    let script_url = "https://opfs-mutation-teardown.test/worker.js";
    let mut first = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            let mutationFile;
            onmessage = () => {
                const movePromise = mutationFile.move("moved.txt");
                postMessage({
                    phase: "move-dispatched",
                    promise: !!movePromise && typeof movePromise.then === "function"
                });
                close();
            };
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                mutationFile = await root.getFileHandle("before.txt", { create: true });
                postMessage({ phase: "ready" });
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store.clone())),
    );

    let ready = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for OPFS mutation setup")
        .expect("worker channel closed before mutation setup");
    assert_eq!(expect_post_json(ready), r#"{"phase":"ready"}"#);

    let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel();
    let (release_blocker_tx, release_blocker_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(
            move |_| {
                blocker_started_tx.send(()).unwrap();
                release_blocker_rx.recv().unwrap();
            },
            |_| {},
        )
        .unwrap();
    blocker_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("storage owner blocker should start");

    first.post_message(serialize_test_string("move"));
    let dispatched = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for pending OPFS move")
        .expect("worker channel closed before move dispatch");
    assert_eq!(
        expect_post_json(dispatched),
        r#"{"phase":"move-dispatched","promise":true}"#
    );
    first.terminate_and_join();

    let (barrier_tx, barrier_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(|_| (), move |result| barrier_tx.send(result).unwrap())
        .unwrap();
    release_blocker_tx
        .send(())
        .expect("storage owner blocker should still be waiting");
    barrier_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("post-mutation storage barrier should finish")
        .expect("post-mutation storage barrier should not panic");

    let mut second = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const moved = await root.getFileHandle("moved.txt");
                const writer = await moved.createWritable({ mode: "exclusive" });
                await writer.write("replacement");
                await writer.close();
                const source = await root.getFileHandle("before.txt").then(
                    () => "present",
                    error => error && error.name
                );
                postMessage({
                    source,
                    text: await (await moved.getFile()).text()
                });
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store)),
    );

    let replacement = timeout(TIMEOUT, second.recv())
        .await
        .expect("timed out waiting for post-teardown OPFS writer")
        .expect("replacement worker channel closed");
    assert_eq!(
        expect_post_json(replacement),
        r#"{"source":"NotFoundError","text":"replacement"}"#
    );
}

#[tokio::test]
async fn worker_opfs_writable_acquisition_and_sink_follow_storage_owner_order() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("worker-writer.txt", { create: true });

            const acquisition = file.createWritable({ mode: "exclusive" });
            const conflictingMove = file.move("blocked.txt");
            const writer = await acquisition;
            const conflict = await outcome(conflictingMove);

            const first = writer.write("W");
            const second = writer.write(new Uint8Array([88]));
            const third = writer.write({ type: "write", position: 2, data: "Y" });
            const closePromise = writer.close();
            const commandPromises = [first, second, third, closePromise].every(
                value => value && typeof value.then === "function"
            );
            await Promise.all([first, second, third, closePromise]);

            await file.move("after.txt");
            const snapshot = await file.getFile();
            postMessage({
                conflict,
                commandPromises,
                finalName: file.name,
                text: await snapshot.text()
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-writable-owner-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"conflict":"NoModificationAllowedError","commandPromises":true,"finalName":"after.txt","text":"WXY"}"#
    );
}

#[tokio::test]
async fn worker_opfs_sync_access_handle_covers_cursor_flush_lock_and_close() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const rejectedName = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const thrownName = callback => {
                try {
                    callback();
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("sync.bin", { create: true });
            const sync = await file.createSyncAccessHandle();
            const shape = [
                sync instanceof FileSystemSyncAccessHandle,
                Object.prototype.toString.call(sync),
                sync.mode,
                ["close", "flush", "getSize", "truncate", "read", "write"]
                    .map(name => typeof sync[name]).join(",")
            ];
            const written = sync.write(new Uint8Array([65, 66, 67]), { at: 2 });
            const sizeAfterWrite = sync.getSize();
            const target = new ArrayBuffer(9);
            new Uint8Array(target).fill(9);
            const view = new Uint8Array(target, 2, 5);
            const read = sync.read(view, { at: 0 });
            const targetBytes = Array.from(new Uint8Array(target));
            sync.truncate(3);
            const sizeAfterTruncate = sync.getSize();
            sync.write(new Uint8Array([90]));
            sync.flush();
            const flushed = Array.from(
                new Uint8Array(await (await file.getFile()).arrayBuffer())
            );
            const lockConflict = await rejectedName(file.createWritable());
            sync.close();
            sync.close();
            const afterClose = [
                thrownName(() => sync.read(new Uint8Array(1))),
                thrownName(() => sync.write(new Uint8Array(1))),
                thrownName(() => sync.flush()),
                thrownName(() => sync.getSize()),
                thrownName(() => sync.truncate(0))
            ];
            const writable = await file.createWritable();
            await writable.write("Q");
            await writable.close();
            const afterUnlock = await new Response(await file.getFile()).text();
            postMessage({
                shape,
                written,
                sizeAfterWrite,
                read,
                targetBytes,
                sizeAfterTruncate,
                flushed,
                lockConflict,
                afterClose,
                afterUnlock
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-sync-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"shape":[true,"[object FileSystemSyncAccessHandle]","readwrite","function,function,function,function,function,function"],"written":3,"sizeAfterWrite":5,"read":5,"targetBytes":[9,9,0,0,65,66,67,9,9],"sizeAfterTruncate":3,"flushed":[0,0,65,90],"lockConflict":"NoModificationAllowedError","afterClose":["InvalidStateError","InvalidStateError","InvalidStateError","InvalidStateError","InvalidStateError"],"afterUnlock":"Q"}"#
    );
}

#[tokio::test]
async fn worker_opfs_sync_shared_modes_enforce_compatibility_and_write_permission() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const rejectedName = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const thrownName = callback => {
                try {
                    callback();
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("shared-sync.bin", { create: true });
            const initialWriter = await file.createWritable();
            await initialWriter.write("initial");
            await initialWriter.close();

            const readOnlyFirst = await file.createSyncAccessHandle({ mode: "read-only" });
            const readOnlySecond = await file.createSyncAccessHandle({ mode: "read-only" });
            const readOnlyConflicts = [
                await rejectedName(file.createSyncAccessHandle({ mode: "readwrite" })),
                await rejectedName(
                    file.createSyncAccessHandle({ mode: "readwrite-unsafe" })),
                await rejectedName(file.createWritable({ mode: "siloed" }))
            ];
            const readOnlyMutations = [
                thrownName(() => readOnlyFirst.write(new Uint8Array([1]))),
                thrownName(() => readOnlyFirst.truncate(0)),
                thrownName(() => readOnlyFirst.flush())
            ];
            const readOnlyBytes = new Uint8Array(7);
            const readOnlyRead = readOnlyFirst.read(readOnlyBytes, { at: 0 });
            readOnlyFirst.close();
            const oneReadOnlyStillLocks = await rejectedName(
                file.createSyncAccessHandle({ mode: "readwrite-unsafe" }));
            readOnlySecond.close();

            const unsafeFirst = await file.createSyncAccessHandle({
                mode: "readwrite-unsafe"
            });
            const unsafeSecond = await file.createSyncAccessHandle({
                mode: "readwrite-unsafe"
            });
            const unsafeConflicts = [
                await rejectedName(file.createSyncAccessHandle({ mode: "read-only" })),
                await rejectedName(file.createSyncAccessHandle({ mode: "readwrite" })),
                await rejectedName(file.createWritable({ mode: "exclusive" }))
            ];
            unsafeFirst.truncate(0);
            unsafeFirst.write(new TextEncoder().encode("first"));
            const unsafeSeenBytes = new Uint8Array(5);
            const unsafeRead = unsafeSecond.read(unsafeSeenBytes, { at: 0 });
            unsafeSecond.write(new TextEncoder().encode("!"), { at: 5 });
            const beforeFlush = await (await file.getFile()).text();
            unsafeFirst.flush();
            const afterFirstFlush = await (await file.getFile()).text();
            unsafeSecond.flush();
            const afterSecondFlush = await (await file.getFile()).text();
            unsafeFirst.close();
            unsafeSecond.close();

            const afterUnlock = await rejectedName(file.createWritable());
            postMessage({
                modes: [
                    readOnlyFirst.mode,
                    readOnlySecond.mode,
                    unsafeFirst.mode,
                    unsafeSecond.mode
                ],
                readOnlyConflicts,
                readOnlyMutations,
                readOnlyRead,
                readOnlyText: new TextDecoder().decode(readOnlyBytes),
                oneReadOnlyStillLocks,
                unsafeConflicts,
                unsafeRead,
                unsafeSeen: new TextDecoder().decode(unsafeSeenBytes),
                beforeFlush,
                afterFirstFlush,
                afterSecondFlush,
                afterUnlock
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-sync-shared-modes-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"modes":["read-only","read-only","readwrite-unsafe","readwrite-unsafe"],"readOnlyConflicts":["NoModificationAllowedError","NoModificationAllowedError","NoModificationAllowedError"],"readOnlyMutations":["NoModificationAllowedError","NoModificationAllowedError","NoModificationAllowedError"],"readOnlyRead":7,"readOnlyText":"initial","oneReadOnlyStillLocks":"NoModificationAllowedError","unsafeConflicts":["NoModificationAllowedError","NoModificationAllowedError","NoModificationAllowedError"],"unsafeRead":5,"unsafeSeen":"first","beforeFlush":"first!","afterFirstFlush":"first!","afterSecondFlush":"first!","afterUnlock":"resolved"}"#
    );
}

#[tokio::test]
async fn worker_opfs_unsafe_handles_keep_separate_cursors_and_enforce_offset_range() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const thrownName = callback => {
                try {
                    callback();
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("unsafe-cursors.bin", { create: true });
            const writer = await file.createWritable();
            await writer.write("abcdef");
            await writer.close();

            const first = await file.createSyncAccessHandle({ mode: "readwrite-unsafe" });
            const second = await file.createSyncAccessHandle({ mode: "readwrite-unsafe" });
            const firstReadBuffer = new Uint8Array(2);
            const secondReadBuffer = new Uint8Array(1);
            const firstRead = first.read(firstReadBuffer, { at: 1 });
            const secondRead = second.read(secondReadBuffer, { at: 5 });
            first.write(new TextEncoder().encode("X"));
            first.truncate(2);
            first.write(new TextEncoder().encode("C"));
            second.write(new TextEncoder().encode("Z"));

            const huge = 2 ** 53;
            const nearLimit = Number.MAX_SAFE_INTEGER;
            const bounds = [
                thrownName(() => first.read(new Uint8Array(0), { at: huge })),
                thrownName(() => first.write(new Uint8Array(0), { at: huge })),
                thrownName(() => first.write(new Uint8Array(2048), { at: nearLimit })),
                thrownName(() => first.truncate(huge))
            ];
            first.write(new Uint8Array(0), { at: nearLimit });
            bounds.push(thrownName(() => first.write(new Uint8Array(2048))));
            const sizes = [first.getSize(), second.getSize()];
            first.close();
            const firstAfterClose = thrownName(() => first.getSize());
            second.write(new TextEncoder().encode("Q"), { at: 0 });
            const liveBytes = Array.from(
                new Uint8Array(await (await file.getFile()).arrayBuffer())
            );
            second.close();

            postMessage({
                modes: [first.mode, second.mode],
                reads: [firstRead, Array.from(firstReadBuffer),
                        secondRead, Array.from(secondReadBuffer)],
                bounds,
                sizes,
                firstAfterClose,
                liveBytes
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-sync-unsafe-cursors-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"modes":["readwrite-unsafe","readwrite-unsafe"],"reads":[2,[98,99],1,[102]],"bounds":["TypeError","TypeError","QuotaExceededError","TypeError","QuotaExceededError"],"sizes":[7,7],"firstAfterClose":"InvalidStateError","liveBytes":[81,98,67,0,0,0,90]}"#
    );
}

#[tokio::test]
async fn worker_opfs_sync_acquisition_follows_storage_owner_order() {
    ensure_v8();
    let mut handle = spawn_worker(
        r#"
        Promise.resolve().then(async () => {
            const outcome = async promise => {
                try {
                    await promise;
                    return "resolved";
                } catch (error) {
                    return error && error.name;
                }
            };
            const root = await navigator.storage.getDirectory();
            const file = await root.getFileHandle("sync-owner.bin", { create: true });

            const acquisition = file.createSyncAccessHandle();
            const conflictingMove = file.move("blocked.bin");
            const sync = await acquisition;
            const conflict = await outcome(conflictingMove);
            const written = sync.write(new Uint8Array([65, 66, 67]));
            sync.close();

            await file.move("after.bin");
            const snapshot = await file.getFile();
            postMessage({
                conflict,
                mode: sync.mode,
                written,
                finalName: file.name,
                bytes: Array.from(new Uint8Array(await snapshot.arrayBuffer()))
            });
            close();
        }).catch(error => {
            postMessage({ errorName: error && error.name, errorMessage: error && error.message });
            close();
        });
        "#
        .into(),
        "https://opfs-sync-owner-worker.test/worker.js".into(),
    );

    let msg = timeout(TIMEOUT, handle.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"conflict":"NoModificationAllowedError","mode":"readwrite","written":3,"finalName":"after.bin","bytes":[65,66,67]}"#
    );
}

#[tokio::test]
async fn worker_teardown_closes_leaked_opfs_sync_handle_and_commits_dirty_data() {
    ensure_v8();
    let bucket_store = crate::new_shared_storage_bucket_store();
    let script_url = "https://opfs-sync-teardown.test/worker.js";
    let mut first = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("leaked-sync.bin", { create: true });
                globalThis.__leakedSync = await file.createSyncAccessHandle();
                globalThis.__leakedSync.write(new Uint8Array([68, 73, 82, 84, 89]));
                postMessage("sync-open");
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store.clone())),
    );

    let opened = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for leaked OPFS sync handle")
        .expect("worker channel closed before sync handle opened");
    assert_eq!(expect_post_json(opened), r#""sync-open""#);
    first.terminate_and_join();

    let mut second = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("leaked-sync.bin");
                const before = Array.from(
                    new Uint8Array(await (await file.getFile()).arrayBuffer())
                );
                const replacement = await file.createSyncAccessHandle();
                replacement.close();
                postMessage({ before, replacementOpened: true });
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store)),
    );

    let replacement = timeout(TIMEOUT, second.recv())
        .await
        .expect("timed out waiting for replacement OPFS sync handle")
        .expect("replacement worker channel closed");
    assert_eq!(
        expect_post_json(replacement),
        r#"{"before":[68,73,82,84,89],"replacementOpened":true}"#
    );
}

#[tokio::test]
async fn worker_context_teardown_aborts_opfs_writable_and_releases_lock() {
    ensure_v8();
    let bucket_store = crate::new_shared_storage_bucket_store();
    let script_url = "https://opfs-writer-teardown.test/worker.js";
    let mut first = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("teardown.txt", { create: true });
                const seed = await file.createWritable();
                await seed.write("committed");
                await seed.close();
                globalThis.__leakedWriter = await file.createWritable({ mode: "exclusive" });
                await globalThis.__leakedWriter.write("must-not-commit");
                postMessage("writer-open");
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store.clone())),
    );

    let msg = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for open OPFS writer")
        .expect("worker channel closed before writer opened");
    assert_eq!(expect_post_json(msg), r#""writer-open""#);
    first.terminate_and_join();

    let mut second = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("teardown.txt");
                const before = await new Response(await file.getFile()).text();
                const replacement = await file.createWritable({ mode: "exclusive" });
                await replacement.write("replacement");
                await replacement.close();
                const after = await new Response(await file.getFile()).text();
                postMessage({ before, after });
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store)),
    );

    let msg = timeout(TIMEOUT, second.recv())
        .await
        .expect("timed out waiting for replacement OPFS writer")
        .expect("worker channel closed before replacement completed");
    assert_eq!(
        expect_post_json(msg),
        r#"{"before":"committed","after":"replacement"}"#
    );
    second.terminate_and_join();
}

#[tokio::test]
async fn worker_teardown_drops_pending_opfs_writable_acquisition_and_releases_lock() {
    ensure_v8();
    let bucket_store = crate::new_shared_storage_bucket_store();
    let storage_service = bucket_store.lock().storage_service();
    let script_url = "https://opfs-acquisition-teardown.test/worker.js";
    let mut first = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            let acquisitionFile;
            onmessage = () => {
                const acquisition = acquisitionFile.createWritable({ mode: "exclusive" });
                postMessage({
                    phase: "acquisition-dispatched",
                    promise: !!acquisition && typeof acquisition.then === "function"
                });
                close();
            };
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                acquisitionFile = await root.getFileHandle(
                    "pending-acquisition.txt",
                    { create: true }
                );
                postMessage({ phase: "ready" });
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store.clone())),
    );

    let ready = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for OPFS acquisition setup")
        .expect("worker channel closed before acquisition setup");
    assert_eq!(expect_post_json(ready), r#"{"phase":"ready"}"#);

    let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel();
    let (release_blocker_tx, release_blocker_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(
            move |_| {
                blocker_started_tx.send(()).unwrap();
                release_blocker_rx.recv().unwrap();
            },
            |_| {},
        )
        .unwrap();
    blocker_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("storage owner blocker should start");

    first.post_message(serialize_test_string("acquire"));
    let dispatched = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for pending OPFS acquisition")
        .expect("worker channel closed before acquisition dispatch");
    assert_eq!(
        expect_post_json(dispatched),
        r#"{"phase":"acquisition-dispatched","promise":true}"#
    );
    first.terminate_and_join();
    let (barrier_tx, barrier_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(|_| (), move |result| barrier_tx.send(result).unwrap())
        .unwrap();
    release_blocker_tx
        .send(())
        .expect("storage owner blocker should still be waiting");

    // This queued barrier runs after the acquisition completion has failed to
    // reach the destroyed Worker and dropped its lease. A following
    // synchronous turn is therefore ordered after the lease's abort ticket.
    barrier_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("post-acquisition storage barrier should finish")
        .expect("post-acquisition storage barrier should not panic");
    storage_service.with_opfs(|_| ());

    let mut second = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("pending-acquisition.txt");
                const replacement = await file.createWritable({ mode: "exclusive" });
                await replacement.write("replacement");
                await replacement.close();
                postMessage({ text: await (await file.getFile()).text() });
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store)),
    );

    let replacement = timeout(TIMEOUT, second.recv())
        .await
        .expect("timed out waiting for replacement OPFS writer")
        .expect("replacement worker channel closed");
    assert_eq!(expect_post_json(replacement), r#"{"text":"replacement"}"#);
}

#[tokio::test]
async fn worker_teardown_drops_pending_opfs_sync_acquisition_and_releases_lock() {
    ensure_v8();
    let bucket_store = crate::new_shared_storage_bucket_store();
    let storage_service = bucket_store.lock().storage_service();
    let script_url = "https://opfs-sync-acquisition-teardown.test/worker.js";
    let mut first = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            let acquisitionFile;
            onmessage = () => {
                const acquisition = acquisitionFile.createSyncAccessHandle();
                postMessage({
                    phase: "acquisition-dispatched",
                    promise: !!acquisition && typeof acquisition.then === "function"
                });
                close();
            };
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                acquisitionFile = await root.getFileHandle(
                    "pending-sync-acquisition.bin",
                    { create: true }
                );
                postMessage({ phase: "ready" });
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store.clone())),
    );

    let ready = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for OPFS sync acquisition setup")
        .expect("worker channel closed before sync acquisition setup");
    assert_eq!(expect_post_json(ready), r#"{"phase":"ready"}"#);

    let (blocker_started_tx, blocker_started_rx) = std::sync::mpsc::channel();
    let (release_blocker_tx, release_blocker_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(
            move |_| {
                blocker_started_tx.send(()).unwrap();
                release_blocker_rx.recv().unwrap();
            },
            |_| {},
        )
        .unwrap();
    blocker_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("storage owner blocker should start");

    first.post_message(serialize_test_string("acquire"));
    let dispatched = timeout(TIMEOUT, first.recv())
        .await
        .expect("timed out waiting for pending OPFS sync acquisition")
        .expect("worker channel closed before sync acquisition dispatch");
    assert_eq!(
        expect_post_json(dispatched),
        r#"{"phase":"acquisition-dispatched","promise":true}"#
    );
    first.terminate_and_join();

    let (barrier_tx, barrier_rx) = std::sync::mpsc::channel();
    storage_service
        .dispatch_opfs(|_| (), move |result| barrier_tx.send(result).unwrap())
        .unwrap();
    release_blocker_tx
        .send(())
        .expect("storage owner blocker should still be waiting");

    // The acquisition completion drops its lease when delivery to the dead
    // Worker fails. Reserve one more synchronous turn after the intervening
    // barrier so the lease's close ticket has committed before replacement.
    barrier_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("post-sync-acquisition storage barrier should finish")
        .expect("post-sync-acquisition storage barrier should not panic");
    storage_service.with_opfs(|_| ());

    let mut second = spawn_test_worker_with_options(
        WorkerSpawnOptions::new(
            r#"
            Promise.resolve().then(async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle("pending-sync-acquisition.bin");
                const replacement = await file.createSyncAccessHandle();
                const written = replacement.write(new Uint8Array([79, 75]));
                replacement.close();
                const bytes = Array.from(
                    new Uint8Array(await (await file.getFile()).arrayBuffer())
                );
                postMessage({ written, bytes });
                close();
            }).catch(error => {
                postMessage({ errorName: error && error.name, errorMessage: error && error.message });
                close();
            });
            "#
            .into(),
            script_url.to_owned(),
        )
        .with_storage_bucket_store(Some(bucket_store)),
    );

    let replacement = timeout(TIMEOUT, second.recv())
        .await
        .expect("timed out waiting for replacement OPFS sync handle")
        .expect("replacement worker channel closed");
    assert_eq!(
        expect_post_json(replacement),
        r#"{"written":2,"bytes":[79,75]}"#
    );
}
