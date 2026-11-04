#ifndef SOLAR_PASS_PLUGIN_H
#define SOLAR_PASS_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SOLAR_PASS_PLUGIN_API_VERSION 1u
#define SOLAR_PASS_PLUGIN_SUCCESS 0u
#define SOLAR_PASS_PLUGIN_FAILURE 1u

#ifndef SOLAR_PASS_PLUGIN_EXPORT
#if defined(_WIN32)
#define SOLAR_PASS_PLUGIN_EXPORT __declspec(dllexport)
#elif defined(__GNUC__)
#define SOLAR_PASS_PLUGIN_EXPORT __attribute__((visibility("default")))
#else
#define SOLAR_PASS_PLUGIN_EXPORT
#endif
#endif

/* Write exactly once, synchronously, on the entry point's calling thread.
 * The host copies the borrowed bytes. Never retain input, context, or writer.
 * A success response is UTF-8 JSON; a failure response is a UTF-8 diagnostic.
 * See the SDK's Request, Manifest, and Response definitions for the wire schema.
 */
typedef uint32_t (*solar_pass_plugin_write_v1)(void *context, const uint8_t *bytes, size_t length);

/* The library exports this unmangled symbol with the platform C calling convention.
 * No exception or panic may cross either ABI boundary. Libraries are trusted code.
 */
SOLAR_PASS_PLUGIN_EXPORT uint32_t solar_pass_plugin_v1(
    const uint8_t *input, size_t length, solar_pass_plugin_write_v1 write, void *context);

#ifdef __cplusplus
}
#endif

#endif
