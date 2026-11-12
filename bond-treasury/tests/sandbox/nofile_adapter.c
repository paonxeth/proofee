/* Test harness ONLY. nearcore requests 65535 FDs at startup. Retain the host's
 * smaller hard limit instead; never raise/bypass a kernel restriction. Suitable
 * for tiny local sandbox fixtures, not a production node. All other calls pass through.
 * No change to WASM, receipts, balances, signatures or consensus execution. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <sys/resource.h>
int setrlimit(__rlimit_resource_t resource, const struct rlimit *requested) {
    int (*original)(__rlimit_resource_t,const struct rlimit*)=dlsym(RTLD_NEXT,"setrlimit");
    struct rlimit host;
    if (resource==RLIMIT_NOFILE && requested->rlim_cur==65535 && requested->rlim_max==65535 && getrlimit(resource,&host)==0 && host.rlim_max<65535) {
        struct rlimit retained={host.rlim_max,host.rlim_max};
        return original(resource,&retained);
    }
    return original(resource,requested);
}
int setrlimit64(__rlimit_resource_t resource, const struct rlimit64 *requested) {
    int (*original)(__rlimit_resource_t,const struct rlimit64*)=dlsym(RTLD_NEXT,"setrlimit64");
    struct rlimit64 host;
    if (resource==RLIMIT_NOFILE && requested->rlim_cur==65535 && requested->rlim_max==65535 && getrlimit64(resource,&host)==0 && host.rlim_max<65535) {
        struct rlimit64 retained={host.rlim_max,host.rlim_max};
        return original(resource,&retained);
    }
    return original(resource,requested);
}
