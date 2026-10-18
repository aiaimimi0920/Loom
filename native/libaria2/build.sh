#!/usr/bin/env bash
# 在已有 WSL MinGW 工具链中构建 Windows C ABI DLL，不安装或升级系统软件。
set -euo pipefail
root="$1"
archive="$2"
bridge="$3"
export SOURCE_DATE_EPOCH=1700000000
mkdir -p "$root"
cd "$root"
tar -xf "$archive"
cd aria2-1.37.0
./configure --host=x86_64-w64-mingw32 --enable-libaria2 --enable-static --disable-shared \
  --disable-bittorrent --disable-metalink --disable-websocket --disable-nls \
  --without-libxml2 --without-libexpat --without-libz --without-libcares \
  --without-sqlite3 --without-libssh2 --without-gnutls --without-openssl \
  --without-libgcrypt --without-libgmp --without-libnettle --with-wintls \
  CXXFLAGS="-Os -ffunction-sections -fdata-sections" > "$root/configure.log" 2>&1
make -C src -j4 libaria2.la > "$root/build.log" 2>&1
cd "$root"
x86_64-w64-mingw32-g++ -std=c++11 -Os -shared -static -static-libgcc -static-libstdc++ \
  -Wl,--no-insert-timestamp -Wl,--gc-sections -Wl,--dynamicbase -Wl,--nxcompat -Wl,-Map,"$root/link.map" \
  -I aria2-1.37.0/src/includes "$bridge" aria2-1.37.0/src/.libs/libaria2.a \
  -lws2_32 -lwsock32 -lgdi32 -lwinmm -liphlpapi -lpsapi -lcrypt32 -lsecur32 -ladvapi32 \
  -o loom_aria2.dll > link.log 2>&1
x86_64-w64-mingw32-strip loom_aria2.dll
x86_64-w64-mingw32-objdump -p loom_aria2.dll > imports.txt
x86_64-w64-mingw32-g++ --version > compiler.txt
