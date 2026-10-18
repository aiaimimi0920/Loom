// libaria2 的 C ABI 边界；调用方必须在同一个专用线程串行调用。
#include <aria2/aria2.h>
#include <cstdint>
#include <memory>
#include <mutex>

using Progress = int (*)(void *, uint64_t, uint64_t, uint64_t);
static std::mutex session_mutex;

struct Library {
  bool initialized = aria2::libraryInit() == 0;
  ~Library() noexcept {
    try {
      if (initialized)
        aria2::libraryDeinit();
    } catch (...) {
    }
  }
};
static Library &global_library() {
  static Library library;
  return library;
}
struct Session {
  aria2::Session *session = nullptr;
  ~Session() noexcept {
    try {
      if (session) {
        aria2::shutdown(session, true);
        while (aria2::run(session, aria2::RUN_ONCE) == 1) {
        }
        aria2::sessionFinal(session);
      }
    } catch (...) {
    }
  }
};

extern "C" __declspec(dllexport) uint32_t loom_aria2_abi() noexcept {
  return 1;
}
extern "C" __declspec(dllexport) void loom_aria2_close() noexcept {
  try {
    std::lock_guard<std::mutex> lock(session_mutex);
    auto &library = global_library();
    if (library.initialized) {
      aria2::libraryDeinit();
      library.initialized = false;
    }
  } catch (...) {
  }
}

// 回调返回 0=继续、1=取消、2=暂停；暂停后返回 0 即恢复。
// 返回 0=完成、1=取消、负数=失败。C++ 异常绝不越过 Rust FFI 边界。
extern "C" __declspec(dllexport) int loom_aria2_download(
    const char *url, const char *directory, Progress progress,
    void *context) noexcept {
  try {
    std::lock_guard<std::mutex> lock(session_mutex);
    if (!url || !directory || !progress)
      return -1;
    auto &global = global_library();
    if (!global.initialized)
      return -2;
    Session library;
    aria2::SessionConfig config;
    config.useSignalHandler = false;
    config.keepRunning = true;
    aria2::KeyVals options = {{"dir", directory},
                              {"out", "payload.zip"},
                              {"quiet", "true"},
                              {"no-netrc", "true"},
                              {"http-proxy", ""},
                              {"https-proxy", ""},
                              {"all-proxy", ""},
                              {"no-proxy", "127.0.0.1,localhost"},
                              {"auto-file-renaming", "false"},
                              {"allow-overwrite", "false"},
                              {"file-allocation", "none"},
                              {"split", "4"},
                              {"max-connection-per-server", "4"},
                              {"min-split-size", "1M"},
                              {"max-tries", "2"},
                              {"connect-timeout", "15"},
                              {"timeout", "30"}};
    library.session = aria2::sessionNew(options, config);
    if (!library.session)
      return -3;
    aria2::A2Gid gid;
    if (aria2::addUri(library.session, &gid, {url}, {{"out", "payload.zip"}}) <
        0)
      return -4;
    bool paused = false;
    bool cancelled = false;
    while (true) {
      int running = aria2::run(library.session, aria2::RUN_ONCE);
      if (running < 0)
        return -5;
      auto *handle = aria2::getDownloadHandle(library.session, gid);
      if (!handle)
        return cancelled ? 1 : -6;
      auto status = handle->getStatus();
      auto total = handle->getTotalLength();
      auto completed = handle->getCompletedLength();
      auto speed = handle->getDownloadSpeed();
      aria2::deleteDownloadHandle(handle);
      // handle 不能跨越下一次 run 存活。
      int control =
          progress(context, total < 0 ? 0 : total,
                   completed < 0 ? 0 : completed, speed < 0 ? 0 : speed);
      if (control == 1 && !cancelled) {
        cancelled = true;
        aria2::shutdown(library.session, true);
      }
      if (cancelled) {
        if (running == 0)
          return 1;
        continue;
      }
      if (status == aria2::DOWNLOAD_COMPLETE)
        return 0;
      if (status == aria2::DOWNLOAD_ERROR || status == aria2::DOWNLOAD_REMOVED)
        return -7;
      if (control == 2 && !paused) {
        if (aria2::pauseDownload(library.session, gid, true) < 0)
          return -8;
        paused = true;
      } else if (control == 0 && paused) {
        if (aria2::unpauseDownload(library.session, gid) < 0)
          return -9;
        paused = false;
      }
      if (running == 0)
        return -10;
    }
  } catch (...) {
    return -11;
  }
}
