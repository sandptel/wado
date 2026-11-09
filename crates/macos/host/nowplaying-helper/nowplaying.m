// wado's reader of the Mac's "now playing" (crates/macos/host/src/nowplaying.rs), built by
// scripts/mac-app.sh into libwado-nowplaying.dylib.
//
// MediaRemote.framework is private, and since macOS 15.4 it serves only Apple's own processes.
// Apple's /usr/bin/perl is one, and loads any library, so wado runs
//   /usr/bin/perl -MDynaLoader -e '<load this and call wado_nowplaying>' <this.dylib> <command>
// with <command> one of:
//   stream                  one JSON object per line, the whole state, on every change
//   play | pause | toggle | next | previous
//   seek <seconds> | shuffle <mode> | repeat <mode>   (modes: 1 off; shuffle 3 on;
//                                                      repeat 2 track, 3 all)

#import <Foundation/Foundation.h>
#import <dlfcn.h>
#import <objc/message.h>

typedef void (^InfoBlock)(NSDictionary *);
typedef void (^BoolBlock)(BOOL);
typedef void (^ClientBlock)(id);
static void (*GetInfo)(dispatch_queue_t, InfoBlock);
static void (*GetIsPlaying)(dispatch_queue_t, BoolBlock);
static void (*GetClient)(dispatch_queue_t, ClientBlock);
static void (*Register)(dispatch_queue_t);
static BOOL (*SendCommand)(int, id);
static void (*SetElapsed)(double);
static void (*SetShuffle)(int);
static void (*SetRepeat)(int);

static dispatch_queue_t queue;

static id k(NSDictionary *d, NSString *key, Class c) {
    id v = d[[@"kMRMediaRemoteNowPlayingInfo" stringByAppendingString:key]];
    return [v isKindOfClass:c] ? v : nil;
}

// A string property of MediaRemote's MRClient, if it has it.
static NSString *ask(id client, SEL sel) {
    id v = [client respondsToSelector:sel] ? ((id (*)(id, SEL))objc_msgSend)(client, sel) : nil;
    return [v isKindOfClass:NSString.class] ? v : nil;
}

// Read everything, then print it unless it is what was printed last.
static void print_state(void) {
    NSMutableDictionary *out = [NSMutableDictionary dictionary];
    dispatch_group_t g = dispatch_group_create();
    dispatch_group_enter(g);
    GetInfo(queue, ^(NSDictionary *i) {
      out[@"title"] = k(i, @"Title", NSString.class);
      out[@"artist"] = k(i, @"Artist", NSString.class);
      out[@"album"] = k(i, @"Album", NSString.class);
      out[@"duration"] = k(i, @"Duration", NSNumber.class);
      out[@"elapsed"] = k(i, @"ElapsedTime", NSNumber.class);
      out[@"rate"] = k(i, @"PlaybackRate", NSNumber.class);
      out[@"shuffle"] = k(i, @"ShuffleMode", NSNumber.class);
      out[@"repeat"] = k(i, @"RepeatMode", NSNumber.class);
      NSDate *at = k(i, @"Timestamp", NSDate.class);
      if (at) out[@"at"] = @(at.timeIntervalSince1970);
      NSData *art = k(i, @"ArtworkData", NSData.class);
      if (art.length) {
          out[@"art"] = [art base64EncodedStringWithOptions:0];
          out[@"artMime"] = k(i, @"ArtworkMIMEType", NSString.class);
      }
      dispatch_group_leave(g);
    });
    dispatch_group_enter(g);
    GetIsPlaying(queue, ^(BOOL playing) {
      out[@"playing"] = @(playing);
      dispatch_group_leave(g);
    });
    dispatch_group_enter(g);
    GetClient(queue, ^(id client) {
      // A web page plays in WebKit's GPU process; the browser is its parent.
      out[@"bundle"] = ask(client, @selector(bundleIdentifier));
      out[@"parent"] = ask(client, @selector(parentApplicationBundleIdentifier));
      dispatch_group_leave(g);
    });
    dispatch_group_notify(g, queue, ^{
      if (!out[@"title"]) [out removeAllObjects];
      NSData *json = [NSJSONSerialization dataWithJSONObject:out options:0 error:nil];
      static NSData *last;
      if (!json || [json isEqual:last]) return;
      last = json;
      fwrite(json.bytes, 1, json.length, stdout);
      fputc('\n', stdout);
      if (fflush(stdout)) exit(0); // wado has gone
    });
}

static void stream(void) {
    __block BOOL pending = NO;
    void (^changed)(NSNotification *) = ^(NSNotification *n) {
      // A change comes as a burst of notifications: read once, after it.
      dispatch_async(queue, ^{
        if (pending) return;
        pending = YES;
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), queue, ^{
          pending = NO;
          print_state();
        });
      });
    };
    for (NSString *name in @[
             @"kMRMediaRemoteNowPlayingInfoDidChangeNotification",
             @"kMRMediaRemoteNowPlayingApplicationDidChangeNotification",
             @"kMRMediaRemoteNowPlayingApplicationIsPlayingDidChangeNotification",
         ]) {
        [NSNotificationCenter.defaultCenter addObserverForName:name
                                                        object:nil
                                                         queue:nil
                                                    usingBlock:changed];
    }
    Register(queue);
    dispatch_async(queue, ^{ print_state(); });
    CFRunLoopRun();
}

// Wait for MediaRemote to answer, so a command is delivered before the process exits.
static void settle(void) {
    dispatch_semaphore_t s = dispatch_semaphore_create(0);
    GetIsPlaying(queue, ^(BOOL _) { dispatch_semaphore_signal(s); });
    dispatch_semaphore_wait(s, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC));
}

static int command(NSString *cmd, NSString *arg) {
    NSDictionary *sends = @{@"play" : @0, @"pause" : @1, @"toggle" : @2, @"next" : @4, @"previous" : @5};
    if (sends[cmd] && !arg) {
        if (!SendCommand([sends[cmd] intValue], nil)) return fprintf(stderr, "%s refused\n", cmd.UTF8String), 1;
    } else if ([cmd isEqual:@"seek"] && arg) {
        SetElapsed(arg.doubleValue);
    } else if ([cmd isEqual:@"shuffle"] && arg) {
        SetShuffle(arg.intValue);
    } else if ([cmd isEqual:@"repeat"] && arg) {
        SetRepeat(arg.intValue);
    } else {
        return fprintf(stderr, "unknown command: %s\n", cmd.UTF8String), 2;
    }
    settle();
    return 0;
}

__attribute__((visibility("default"))) void wado_nowplaying(void) {
    void *mr = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY);
    GetInfo = dlsym(mr, "MRMediaRemoteGetNowPlayingInfo");
    GetIsPlaying = dlsym(mr, "MRMediaRemoteGetNowPlayingApplicationIsPlaying");
    GetClient = dlsym(mr, "MRMediaRemoteGetNowPlayingClient");
    Register = dlsym(mr, "MRMediaRemoteRegisterForNowPlayingNotifications");
    SendCommand = dlsym(mr, "MRMediaRemoteSendCommand");
    SetElapsed = dlsym(mr, "MRMediaRemoteSetElapsedTime");
    SetShuffle = dlsym(mr, "MRMediaRemoteSetShuffleMode");
    SetRepeat = dlsym(mr, "MRMediaRemoteSetRepeatMode");
    if (!(GetInfo && GetIsPlaying && GetClient && Register && SendCommand && SetElapsed && SetShuffle && SetRepeat)) {
        fprintf(stderr, "MediaRemote: %s\n", dlerror() ?: "a function is missing");
        exit(1);
    }
    queue = dispatch_queue_create("wado.nowplaying", DISPATCH_QUEUE_SERIAL);

    // Our arguments are the process's, after this library's path.
    Dl_info me;
    dladdr((void *)wado_nowplaying, &me);
    NSArray<NSString *> *argv = NSProcessInfo.processInfo.arguments;
    NSUInteger i = [argv indexOfObject:@(me.dli_fname)];
    NSArray *args = i == NSNotFound ? @[] : [argv subarrayWithRange:NSMakeRange(i + 1, argv.count - i - 1)];
    if (args.count == 1 && [args[0] isEqual:@"stream"]) stream();
    if (args.count < 1 || args.count > 2) {
        fprintf(stderr, "usage: stream | play | pause | toggle | next | previous | seek <s> | shuffle <m> | repeat <m>\n");
        exit(2);
    }
    exit(command(args[0], args.count > 1 ? args[1] : nil));
}
