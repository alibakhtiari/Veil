# Phase-3 skeleton: links aether_jni.c against the per-ABI libaether.so.
#
# libaether.so is NOT built here: it comes from the existing
# release.yml:android job (NDK r26d, platform 24, cargo-ndk) and is
# checked into app/src/main/jniLibs/<abi>/libaether.so, one per ABI:
# arm64-v8a, armeabi-v7a, x86_64.
#
# Wire into app/build.gradle (android block):
#
#   externalNativeBuild {
#     cmake {
#       path "src/main/cpp/CMakeLists.txt"
#       version "3.22.1+"
#     }
#   }

cmake_minimum_required(VERSION 3.22.1)

project("aether_jni" C ASM)
enable_language(ASM)

# Prebuilt core, one .so per ABI under jniLibs.
add_library(aether SHARED IMPORTED)
set_target_properties(aether PROPERTIES
    IMPORTED_LOCATION
    ${CMAKE_SOURCE_DIR}/../jniLibs/${ANDROID_ABI}/libaether.so)

add_library(aether_jni SHARED aether_jni.c)

target_include_directories(aether_jni PRIVATE ${CMAKE_SOURCE_DIR})

find_library(log-lib log)

add_subdirectory(hev)

target_link_libraries(aether_jni aether hev-socks5-tunnel ${log-lib})
