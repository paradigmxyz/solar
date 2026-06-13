(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.SolarSoljson = factory();
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  "use strict";

  function setupMethods(soljson) {
    soljson = soljson || {};
    const lowlevel = createLowlevel(soljson);
    const features = createFeatures(lowlevel);
    const methods = {
      compile(inputJsonString, callbacks) {
        if (typeof lowlevel.compileStandard !== "function") {
          throw new Error("solidity_compile is not available");
        }
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
      reset: soljson.reset || exportedFunction(soljson, "solidity_reset") || null,
    };
  }

  function createFeatures(lowlevel) {
    const hasStandardJson = typeof lowlevel.compileStandard === "function";
    return {
      legacySingleInput: false,
      multipleInputs: hasStandardJson,
      importCallback: hasStandardJson,
      nativeStandardJSON: hasStandardJson,
    };
  }

  function createCAbiCompileStandard(soljson) {
    const compile = exportedFunction(soljson, "solidity_compile");
    const alloc = exportedFunction(soljson, "solidity_alloc");
    const free = exportedFunction(soljson, "solidity_free");
    const reset = exportedFunction(soljson, "solidity_reset");
    if (!compile || !alloc || !free) {
      return null;
    }

    return function compileStandard(inputJsonString, callbacks) {
      if (callbacks != null && typeof callbacks !== "object") {
        throw new Error("Invalid callback object specified.");
      }
      callbacks = callbacks || {};
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
        if (callbackPtr) {
          removeFunction(soljson, callbackPtr);
        }
        if (typeof reset === "function") {
          reset();
        }
      }
    };
  }

  function makeReadCallback(soljson, alloc, callbacks) {
    if (!canAddFunction(soljson)) {
      return 0;
    }
    return addFunction(soljson, function (_context, kindPtr, dataPtr, contentsPtr, errorPtr) {
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
    }, "viiiii");
  }

  function handleReadCallback(kind, data, callbacks) {
    if (kind === "source") {
      return normalizeCallbackResult(
        (callbacks.import || defaultImportCallback)(data),
        "File import callback returned no contents",
      );
    }
    if (kind === "smt-query") {
      return normalizeCallbackResult(
        (callbacks.smtSolver || defaultSmtSolverCallback)(data),
        "SMT solver callback returned no contents",
      );
    }
    return { error: `Callback kind \`${kind}\` is not supported` };
  }

  function normalizeCallbackResult(result, missingMessage) {
    if (typeof result === "string") {
      return { contents: result };
    }
    if (result && result.contents != null) {
      return { contents: String(result.contents) };
    }
    if (result && result.error != null) {
      return { error: String(result.error) };
    }
    return { error: missingMessage };
  }

  function defaultImportCallback() {
    return { error: "File import callback not supported" };
  }

  function defaultSmtSolverCallback() {
    return { error: "SMT solver callback not supported" };
  }

  function canAddFunction(soljson) {
    return (
      typeof soljson.addFunction === "function" ||
      !!(soljson.Runtime && typeof soljson.Runtime.addFunction === "function")
    );
  }

  function addFunction(soljson, callback, signature) {
    if (typeof soljson.addFunction === "function") {
      return soljson.addFunction(callback, signature);
    }
    return soljson.Runtime.addFunction(callback, signature);
  }

  function removeFunction(soljson, callbackPtr) {
    if (typeof soljson.removeFunction === "function") {
      soljson.removeFunction(callbackPtr);
    } else if (soljson.Runtime && typeof soljson.Runtime.removeFunction === "function") {
      soljson.Runtime.removeFunction(callbackPtr);
    }
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
    value = String(value);
    if (typeof soljson.lengthBytesUTF8 === "function" && typeof soljson.stringToUTF8 === "function") {
      const length = soljson.lengthBytesUTF8(value) + 1;
      const ptr = alloc(length);
      soljson.stringToUTF8(value, ptr, length);
      return ptr;
    }
    const bytes = textEncoder().encode(value + "\0");
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
    if (typeof soljson.Pointer_stringify === "function") {
      return soljson.Pointer_stringify(ptr);
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
