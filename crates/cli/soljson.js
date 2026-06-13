(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.SolarSoljson = factory();
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  "use strict";

  const features = {
    legacySingleInput: false,
    multipleInputs: true,
    importCallback: true,
    nativeStandardJSON: true,
  };

  function setupMethods(soljson) {
    soljson = soljson || {};
    const lowlevel = createLowlevel(soljson);
    const methods = {
      compile(inputJsonString, callbacks) {
        return lowlevel.compileStandard(inputJsonString, callbacks);
      },
      version() {
        return lowlevel.version();
      },
      semver() {
        if (typeof lowlevel.semver === "function") {
          return lowlevel.semver();
        }
        const match = String(lowlevel.version()).match(/\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?/);
        return match ? match[0] : lowlevel.version();
      },
      license() {
        return lowlevel.license();
      },
      features,
      lowlevel,
      setupMethods,
    };
    return methods;
  }

  function createLowlevel(soljson) {
    const compileStandard =
      soljson.compileStandard ||
      (soljson.lowlevel && soljson.lowlevel.compileStandard) ||
      createCAbiCompileStandard(soljson);

    return {
      compileStandard,
      compileSingle: null,
      compileMulti: null,
      compileCallback: null,
      license: soljson.license || cStringFunction(soljson, "solidity_license", "MIT OR Apache-2.0"),
      version: soljson.version || cStringFunction(soljson, "solidity_version", "unknown"),
      semver: soljson.semver || null,
    };
  }

  function createCAbiCompileStandard(soljson) {
    const compile = exportedFunction(soljson, "solidity_compile");
    const alloc = exportedFunction(soljson, "solidity_alloc");
    const free = exportedFunction(soljson, "solidity_free");
    if (!compile || !alloc || !free) {
      return function missingCompileStandard() {
        throw new Error("solidity_compile is not available");
      };
    }

    return function compileStandard(inputJsonString, callbacks) {
      const inputPtr = allocateString(soljson, alloc, inputJsonString);
      const callbackPtr = makeReadCallback(soljson, alloc, callbacks);
      try {
        const outputPtr = compile(inputPtr, callbackPtr || 0, 0);
        try {
          return readString(soljson, outputPtr);
        } finally {
          free(outputPtr);
        }
      } finally {
        free(inputPtr);
        if (callbackPtr && typeof soljson.removeFunction === "function") {
          soljson.removeFunction(callbackPtr);
        }
      }
    };
  }

  function makeReadCallback(soljson, alloc, callbacks) {
    if (!callbacks || typeof soljson.addFunction !== "function") {
      return 0;
    }
    return soljson.addFunction(function (_context, kindPtr, dataPtr, contentsPtr, errorPtr) {
      const result = handleReadCallback(
        readString(soljson, kindPtr),
        readString(soljson, dataPtr),
        callbacks,
      );
      if (result.contents != null) {
        setPointer(soljson, contentsPtr, allocateString(soljson, alloc, result.contents));
      } else if (result.error != null) {
        setPointer(soljson, errorPtr, allocateString(soljson, alloc, result.error));
      }
    }, "vppppp");
  }

  function handleReadCallback(kind, data, callbacks) {
    if (kind === "source") {
      if (!callbacks || typeof callbacks.import !== "function") {
        return { error: "File import callback not supported" };
      }
      const result = callbacks.import(data);
      if (typeof result === "string") {
        return { contents: result };
      }
      if (result && result.contents != null) {
        return { contents: String(result.contents) };
      }
      if (result && result.error != null) {
        return { error: String(result.error) };
      }
      return { error: "File import callback returned no contents" };
    }
    return { error: `Callback kind \`${kind}\` is not supported` };
  }

  function cStringFunction(soljson, name, fallback) {
    const fn = exportedFunction(soljson, name);
    if (!fn) {
      return function () {
        return fallback;
      };
    }
    return function () {
      return readString(soljson, fn());
    };
  }

  function exportedFunction(soljson, name) {
    return (
      soljson[name] ||
      soljson["_" + name] ||
      (soljson.instance && soljson.instance.exports && soljson.instance.exports[name]) ||
      (soljson.exports && soljson.exports[name]) ||
      null
    );
  }

  function allocateString(soljson, alloc, value) {
    const bytes = textEncoder().encode(String(value) + "\0");
    const ptr = alloc(bytes.length);
    heap(soljson).set(bytes, ptr);
    return ptr;
  }

  function readString(soljson, ptr) {
    if (!ptr) {
      return "";
    }
    if (typeof soljson.UTF8ToString === "function") {
      return soljson.UTF8ToString(ptr);
    }
    const bytes = heap(soljson);
    let end = ptr;
    while (bytes[end] !== 0) {
      end++;
    }
    return textDecoder().decode(bytes.subarray(ptr, end));
  }

  function setPointer(soljson, ptr, value) {
    if (typeof soljson.setValue === "function") {
      soljson.setValue(ptr, value, "*");
      return;
    }
    new DataView(heap(soljson).buffer).setUint32(ptr, value, true);
  }

  function heap(soljson) {
    if (soljson.HEAPU8) {
      return soljson.HEAPU8;
    }
    const memory =
      soljson.memory ||
      soljson.wasmMemory ||
      (soljson.instance && soljson.instance.exports && soljson.instance.exports.memory) ||
      (soljson.exports && soljson.exports.memory);
    if (!memory) {
      throw new Error("WASM memory is not available");
    }
    return new Uint8Array(memory.buffer);
  }

  let encoder;
  function textEncoder() {
    encoder = encoder || new TextEncoder();
    return encoder;
  }

  let decoder;
  function textDecoder() {
    decoder = decoder || new TextDecoder("utf-8");
    return decoder;
  }

  return setupMethods({});
});
