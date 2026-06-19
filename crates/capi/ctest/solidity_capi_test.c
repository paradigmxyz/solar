#include "libsolc.h"

#include <stdio.h>
#include <string.h>

static int fail(char const* message)
{
	fprintf(stderr, "%s\n", message);
	return 1;
}

static char* capi_strdup(char const* value)
{
	size_t length = strlen(value);
	char* out = solidity_alloc(length + 1);
	if (out == NULL)
		return NULL;
	memcpy(out, value, length + 1);
	return out;
}

static void read_callback(
	void* context,
	char const* kind,
	char const* data,
	char** contents,
	char** error
)
{
	(void)context;
	*contents = NULL;
	*error = NULL;

	if (strcmp(kind, "source") == 0 && strcmp(data, "B.sol") == 0)
	{
		*contents = capi_strdup("contract B {}");
		return;
	}

	*error = capi_strdup("source not found");
}

int main(void)
{
	if (solidity_license() == NULL || strlen(solidity_license()) == 0)
		return fail("missing license");
	if (solidity_version() == NULL || strlen(solidity_version()) == 0)
		return fail("missing version");

	char const* input =
		"{\"language\":\"Solidity\","
		"\"sources\":{\"A.sol\":{\"content\":\"contract A { function answer() public pure returns (uint256) { return 42; } }\"}},"
		"\"settings\":{\"outputSelection\":{\"*\":{\"*\":[\"evm.methodIdentifiers\"]}}}}";
	char* output = solidity_compile(input, NULL, NULL);
	if (output == NULL)
		return fail("compile returned NULL");
	if (strstr(output, "\"answer()\":\"85bb7d69\"") == NULL)
	{
		fprintf(stderr, "%s\n", output);
		solidity_free(output);
		return fail("missing method identifier");
	}
	solidity_free(output);

	char const* imported_input =
		"{\"language\":\"Solidity\","
		"\"sources\":{\"A.sol\":{\"content\":\"import \\\"B.sol\\\"; contract A is B {}\"}},"
		"\"settings\":{\"outputSelection\":{\"*\":{\"*\":[\"abi\"]}}}}";
	output = solidity_compile(imported_input, read_callback, NULL);
	if (output == NULL)
		return fail("callback compile returned NULL");
	if (strstr(output, "\"A\"") == NULL || strstr(output, "\"B\"") == NULL)
	{
		fprintf(stderr, "%s\n", output);
		solidity_free(output);
		return fail("missing callback output contracts");
	}
	solidity_free(output);

	char* allocation = solidity_alloc(32);
	if (allocation == NULL)
		return fail("solidity_alloc returned NULL");
	memcpy(allocation, "reset-owned allocation", 23);
	solidity_reset();

	return 0;
}
