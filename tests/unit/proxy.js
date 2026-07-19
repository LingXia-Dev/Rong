describe("Proxy", () => {
  describe("host-created object proxies", () => {
    test("intercepts property reads and preserves object semantics", () => {
      const target = {
        value: 41,
        label: "kept",
      };

      const proxy = createHostProxy(target, {
        get(innerTarget, key, receiver) {
          if (key === "value") {
            return 42;
          }
          return Reflect.get(innerTarget, key, receiver);
        },
      });

      expect(isHostProxy(proxy)).toBe(true);
      expect(isHostProxy(target)).toBe(false);
      expect(typeof proxy).toBe("object");
      expect(proxy.value).toBe(42);
      expect(proxy.label).toBe("kept");
      expect(getHostProxyTarget(proxy)).toBe(target);
    });

    test("returns the original target identity", () => {
      const target = { id: "target" };
      const proxy = createHostProxy(target, {});

      const observedTarget = getHostProxyTarget(proxy);
      expect(observedTarget).toBe(target);
      expect(observedTarget.id).toBe("target");
    });

    test("throws when getting target from a non-proxy value", () => {
      let message = "";

      try {
        getHostProxyTarget({ plain: true });
      } catch (error) {
        message = error.message;
      }

      expect(message).toContain("Not JS Proxy");
    });

    test("supports set traps and writes through to the backing target", () => {
      const writes = [];
      const target = { count: 1 };
      const proxy = createHostProxy(target, {
        set(innerTarget, key, value) {
          writes.push(`${String(key)}:${value}`);
          innerTarget[key] = value + 1;
          return true;
        },
      });

      proxy.count = 4;

      expect(proxy.count).toBe(5);
      expect(target.count).toBe(5);
      expect(writes).toEqual(["count:4"]);
    });

    test("supports has, ownKeys and deleteProperty traps", () => {
      const target = {
        visible: 1,
        secret: 2,
        removable: true,
      };

      const proxy = createHostProxy(target, {
        has(innerTarget, key) {
          if (key === "secret") {
            return false;
          }
          return Reflect.has(innerTarget, key);
        },
        ownKeys() {
          return ["visible", "removable"];
        },
        getOwnPropertyDescriptor(innerTarget, key) {
          if (key === "visible" || key === "removable") {
            return {
              configurable: true,
              enumerable: true,
              writable: true,
              value: innerTarget[key],
            };
          }
          return Reflect.getOwnPropertyDescriptor(innerTarget, key);
        },
        deleteProperty(innerTarget, key) {
          if (key === "removable") {
            return Reflect.deleteProperty(innerTarget, key);
          }
          return false;
        },
      });

      expect("visible" in proxy).toBe(true);
      expect("secret" in proxy).toBe(false);
      expect(Object.keys(proxy)).toEqual(["visible", "removable"]);
      expect(delete proxy.removable).toBe(true);
      expect(target.removable).toBeUndefined();
    });

    test("propagates handler-thrown errors", () => {
      const proxy = createHostProxy({}, {
        get() {
          throw new Error("boom:get");
        },
      });

      let message = "";
      try {
        void proxy.anything;
      } catch (error) {
        message = error.message;
      }

      expect(message).toBe("boom:get");
    });
  });

  describe("host-created function proxies", () => {
    test("supports apply traps and preserves function typeof", () => {
      function target(prefix, value) {
        return `${prefix}:${value}`;
      }

      const proxy = createHostProxy(target, {
        apply(innerTarget, thisArg, args) {
          return Reflect.apply(innerTarget, thisArg, [
            args[0].toUpperCase(),
            args[1] * 2,
          ]);
        },
      });

      expect(isHostProxy(proxy)).toBe(true);
      expect(typeof proxy).toBe("function");
      expect(proxy("sum", 5)).toBe("SUM:10");
      expect(getHostProxyTarget(proxy)).toBe(target);
      expect(getHostProxyTarget(proxy)("raw", 3)).toBe("raw:3");
    });

    test("can gate method calls through get and nested apply traps", () => {
      const allow = {
        xx: true,
        yy: false,
      };

      const functions = {
        xx() {
          return "ok";
        },
        yy() {
          return "blocked";
        },
      };

      const proxy = createHostProxy(functions, {
        get(target, key, receiver) {
          const value = Reflect.get(target, key, receiver);
          if (typeof value !== "function") {
            return value;
          }

          return new Proxy(value, {
            apply(innerTarget, thisArg, args) {
              if (!allow[key]) {
                throw new Error(`blocked:${String(key)}`);
              }
              return Reflect.apply(innerTarget, target, args);
            },
          });
        },
      });

      expect(proxy.xx()).toBe("ok");

      let message = "";
      try {
        proxy.yy();
      } catch (error) {
        message = error.message;
      }

      expect(message).toBe("blocked:yy");
    });

    test("supports construct traps for constructor functions", () => {
      function Person(name) {
        this.name = name;
      }

      const proxy = createHostProxy(Person, {
        construct(innerTarget, args) {
          return {
            viaProxy: true,
            sourceName: innerTarget.name,
            name: String(args[0]).toUpperCase(),
          };
        },
      });

      const instance = new proxy("alice");

      expect(isHostProxy(proxy)).toBe(true);
      expect(typeof proxy).toBe("function");
      expect(instance.viaProxy).toBe(true);
      expect(instance.sourceName).toBe("Person");
      expect(instance.name).toBe("ALICE");
    });
  });

  describe("plain JavaScript proxies", () => {
    test("still behave normally in engine-level JS semantics", () => {
      const target = { count: 1 };
      const proxy = new Proxy(target, {
        get(innerTarget, key, receiver) {
          if (key === "count") {
            return 2;
          }
          return Reflect.get(innerTarget, key, receiver);
        },
      });

      expect(isHostProxy(proxy)).toBe(true);
      expect(typeof proxy).toBe("object");
      expect(proxy.count).toBe(2);
      expect(target.count).toBe(1);
      expect(getHostProxyTarget(proxy)).toBe(target);
    });
  });
});
