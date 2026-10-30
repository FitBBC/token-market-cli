// Opt-in fixture for macos-local-signing.py smoke. Never searches the login keychain.
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdio.h>
#include <string.h>

#ifndef VARIANT
#define VARIANT 0
#endif

static const char *service = "cn.tokenmarket.fixture.sharing";
static const char *account = "fixture-only";
static const char *initial = "dummy-session-not-a-token";
static const char *updated = "dummy-renewed-session-not-a-token";

static int failed(OSStatus status) {
    if (status == errSecInteractionNotAllowed || status == errSecAuthFailed) return 10;
    fprintf(stderr, "fixture Security status: %d (variant %d)\n", (int)status, VARIANT);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    if (SecKeychainSetUserInteractionAllowed(false) != errSecSuccess) return 3;
    SecKeychainRef keychain = NULL;
    OSStatus status;
    if (strcmp(argv[1], "create") == 0) {
        // Native create temporarily adds to search list. Restore the exact list
        // immediately, before returning, and never make this keychain default.
        CFArrayRef search = NULL;
        status = SecKeychainCopySearchList(&search);
        if (status != errSecSuccess) return failed(status);
        const char *password = "throwaway-keychain-fixture";
        status = SecKeychainCreate(argv[2], (UInt32)strlen(password), password, false, NULL, &keychain);
        OSStatus restored = SecKeychainSetSearchList(search);
        CFRelease(search);
        if (status != errSecSuccess) return failed(status);
        if (restored != errSecSuccess) return failed(restored);
        status = SecKeychainAddGenericPassword(keychain, (UInt32)strlen(service), service,
                    (UInt32)strlen(account), account, (UInt32)strlen(initial), initial, NULL);
        CFRelease(keychain);
        return status == errSecSuccess ? 0 : failed(status);
    }
    status = SecKeychainOpen(argv[2], &keychain);
    if (status != errSecSuccess) return failed(status);
    const char *password = "throwaway-keychain-fixture";
    status = SecKeychainUnlock(keychain, (UInt32)strlen(password), password, true);
    if (status != errSecSuccess) return failed(status);
    if (strcmp(argv[1], "delete") == 0) {
        status = SecKeychainDelete(keychain);
        CFRelease(keychain);
        return status == errSecSuccess ? 0 : failed(status);
    }
    const void *values[] = { keychain };
    CFArrayRef search = CFArrayCreate(NULL, values, 1, &kCFTypeArrayCallBacks);
    UInt32 length = 0;
    void *bytes = NULL;
    SecKeychainItemRef item = NULL;
    status = SecKeychainFindGenericPassword(search, (UInt32)strlen(service), service,
                (UInt32)strlen(account), account, &length, &bytes, &item);
    CFRelease(search);
    CFRelease(keychain);
    if (status != errSecSuccess) return failed(status);
    const char *expected = strcmp(argv[1], "read-updated") == 0 ? updated : initial;
    int matched = length == strlen(expected) && memcmp(bytes, expected, length) == 0;
    SecKeychainItemFreeContent(NULL, bytes);
    if (!matched) { CFRelease(item); return 4; }
    if (strcmp(argv[1], "update") == 0)
        status = SecKeychainItemModifyAttributesAndData(item, NULL, (UInt32)strlen(updated), updated);
    CFRelease(item);
    return status == errSecSuccess ? 0 : failed(status);
}
