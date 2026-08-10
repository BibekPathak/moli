(function () {
  const config = globalThis.__lmChildBrowsingContextCurrentEntryChangeConfig;
  if (!config) {
    return;
  }

  function wait(ms) {
    return new Promise(function (resolve) {
      setTimeout(resolve, ms);
    });
  }

  function wait_for(promise, description) {
    return Promise.race([
      promise,
      wait(1000).then(function () {
        throw new Error("timed out waiting for " + description);
      }),
    ]);
  }

  function wait_for_child_commit(iframe, childBaseUrl) {
    return wait_for(
      new Promise(function (resolve) {
        function poll() {
          const childWindow = iframe.contentWindow;
          if (
            childWindow &&
            childWindow.location.href === childBaseUrl &&
            childWindow.navigation.currentEntry?.url === childBaseUrl
          ) {
            resolve();
            return;
          }
          setTimeout(poll, 10);
        }
        poll();
      }),
      "child iframe initial navigation commit",
    );
  }

  promise_test(async function () {
    const iframe = document.getElementById("child");
    const childBaseUrl = new URL(iframe.getAttribute("src"), location.href).href;
    await wait_for_child_commit(iframe, childBaseUrl);

    const childWindow = iframe.contentWindow;
    if (config.mode === "window-local") {
      await assert_window_local_currententrychange(childWindow, childBaseUrl);
      return;
    }

    await assert_forward_traverse_currententrychange(childWindow, childBaseUrl);
  }, config.testName);

  async function assert_window_local_currententrychange(childWindow, childBaseUrl) {
    let topChanges = 0;
    let childChanges = 0;
    let childPropertyChanges = 0;
    const childUrls = [];
    const childPropertyUrls = [];
    const childFromUrls = [];
    const childPropertyFromUrls = [];
    const childNavigationTypes = [];
    const childPropertyNavigationTypes = [];

    window.navigation.addEventListener("currententrychange", function () {
      topChanges += 1;
    });

    childWindow.navigation.addEventListener("currententrychange", function (event) {
      childChanges += 1;
      childUrls.push(childWindow.navigation.currentEntry?.url ?? "null");
      childFromUrls.push(event.from?.url ?? "null");
      childNavigationTypes.push(String(event.navigationType));
    });

    childWindow.navigation.oncurrententrychange = function (event) {
      childPropertyChanges += 1;
      childPropertyUrls.push(childWindow.navigation.currentEntry?.url ?? "null");
      childPropertyFromUrls.push(event.from?.url ?? "null");
      childPropertyNavigationTypes.push(String(event.navigationType));
    };

    childWindow.navigation.navigate("#one", {
      history: "push",
      state: { from: "child" },
    });

    await wait(0);

    assert_equals(
      topChanges,
      0,
      "child currententrychange should stay window-local to the child browsing context",
    );
    assert_equals(childChanges, 1, "child listener should fire once");
    assert_equals(childPropertyChanges, 1, "child oncurrententrychange should fire once");
    assert_equals(
      childUrls.join("|"),
      childBaseUrl + "#one",
      "child listener should observe the committed child currentEntry URL",
    );
    assert_equals(
      childPropertyUrls.join("|"),
      childBaseUrl + "#one",
      "child property listener should observe the committed child currentEntry URL",
    );
    assert_equals(
      childFromUrls.join("|"),
      childBaseUrl,
      "child listener event.from should point at the pre-navigation child entry",
    );
    assert_equals(
      childPropertyFromUrls.join("|"),
      childBaseUrl,
      "child property event.from should point at the pre-navigation child entry",
    );
    assert_equals(
      childNavigationTypes.join("|"),
      "push",
      "child listener navigationType should be push",
    );
    assert_equals(
      childPropertyNavigationTypes.join("|"),
      "push",
      "child property listener navigationType should be push",
    );
    assert_equals(
      childWindow.navigation.currentEntry?.url ?? "null",
      childBaseUrl + "#one",
      "child currentEntry should land on the pushed fragment entry",
    );
  }

  async function assert_forward_traverse_currententrychange(childWindow, childBaseUrl) {
    let topChanges = 0;

    window.navigation.addEventListener("currententrychange", function () {
      topChanges += 1;
    });

    childWindow.history.pushState({ n: 1 }, "", "#one");
    childWindow.history.pushState({ n: 2 }, "", "#two");
    await wait(0);

    const snapshot = await wait_for(
      new Promise(function (resolve) {
        childWindow.addEventListener(
          "hashchange",
          function onHashchange() {
            if (childWindow.location.hash !== "#one") {
              return;
            }

            childWindow.removeEventListener("hashchange", onHashchange);

            const order = [];
            let fired = false;
            let fromUrl = "null";
            let navigationType = "null";
            let syncHash = "";
            let syncState = "";

            childWindow.navigation.oncurrententrychange = function (event) {
              fired = true;
              fromUrl = event.from?.url ?? "null";
              navigationType = String(event.navigationType);
              order.push("currententrychange");
              queueMicrotask(function () {
                order.push("currententrychange-microtask:" + childWindow.location.hash);
              });
              setTimeout(function () {
                resolve({
                  fired: String(fired),
                  topChanges: String(topChanges),
                  syncHash: syncHash,
                  syncState: syncState,
                  timeoutHash: childWindow.location.hash,
                  timeoutState: JSON.stringify(childWindow.history.state),
                  fromUrl: fromUrl,
                  navigationType: navigationType,
                  order: order.join(","),
                });
              }, 0);
            };

            childWindow.navigation.forward();
            syncHash = childWindow.location.hash;
            syncState = JSON.stringify(childWindow.history.state);
          },
          { once: false },
        );

        childWindow.history.back();
      }),
      "child forward currententrychange traverse snapshot",
    );

    assert_equals(
      snapshot.topChanges,
      "0",
      "child traversal currententrychange should stay window-local to the child browsing context",
    );
    assert_equals(
      snapshot.fired,
      "true",
      "child oncurrententrychange should fire for forward traversal",
    );
    assert_equals(
      snapshot.syncHash,
      "#one",
      "child forward traversal should remain asynchronous during the initiating call",
    );
    assert_equals(
      snapshot.syncState,
      "{\"n\":1}",
      "child history.state should still reflect the pre-traversal entry synchronously",
    );
    assert_equals(
      snapshot.timeoutHash,
      "#two",
      "child forward traversal should eventually restore the second fragment",
    );
    assert_equals(
      snapshot.timeoutState,
      "{\"n\":2}",
      "child history.state should eventually restore the second traversal state",
    );
    assert_equals(
      snapshot.fromUrl,
      childBaseUrl + "#one",
      "child oncurrententrychange event.from should point at the pre-traversal entry",
    );
    assert_equals(
      snapshot.navigationType,
      "traverse",
      "child oncurrententrychange should surface navigationType=traverse",
    );
    assert_equals(
      snapshot.order,
      "currententrychange,currententrychange-microtask:#two",
      "child oncurrententrychange should observe the traversed fragment by microtask time",
    );
  }
})();
