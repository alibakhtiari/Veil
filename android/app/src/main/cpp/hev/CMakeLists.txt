# hev-socks5-tunnel vendored build.
#
# Upstream: https://github.com/heiher/hev-socks5-tunnel
# Pinned revision: tag 2.9.3
#   commit b9b9b7b9b0febe32bb5d8cdb9ffa414d94242b75 (2025-03-14)
#   submodules pinned by the superproject:
#     src/core                    1.3.7   (5d37a81d)
#     third-part/hev-task-system  5.6.1   (ed320fd8)
#     third-part/lwip             2.2.1.1 (7a246bcd)
#     third-part/yaml             0.2.5.2 (efa36117)
# License: MIT (see "License"; third-part/lwip is BSD-style, see its
#   License). Both permit redistribution; keep the License files in place.
#
# NOTE: upstream ships no CMakeLists.txt (Makefile + Android.mk only), so
# this file mirrors its Android.mk module list: yaml / hev-task-system /
# lwip as STATIC helpers, and hev-socks5-tunnel as a STATIC archive linked
# into libaether_jni.so. Source selection mirrors upstream build.mk (every
# *.c plus the single arch-dispatching *.S under each module's src/), with
# two deliberate deviations:
#   1. src/hev-jni.c is EXCLUDED: its JNI_OnLoad registers natives for the
#      upstream sample class (hev/htproxy.TProxyService), which does not
#      exist here. Our bindings live in ../aether_jni.c.
#   2. hev-main.c is kept whole: its weak main() is harmless in a shared
#      object, and it defines hev_socks5_tunnel_main_from_str, which the
#      JNI glue uses.

cmake_minimum_required(VERSION 3.22.1)

project("hev_vendor" C ASM)
enable_language(ASM)

if(ANDROID_ABI STREQUAL "armeabi-v7a")
    add_compile_options(-mfpu=neon)
endif()

# --- yaml (mirrors third-part/yaml/Android.mk + configs.mk) ---
file(GLOB_RECURSE YAML_SRCS "${CMAKE_CURRENT_SOURCE_DIR}/third-part/yaml/src/*.c")
add_library(yaml STATIC ${YAML_SRCS})
target_include_directories(yaml PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/yaml/src"
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/yaml/include")
target_compile_definitions(yaml PUBLIC
    YAML_VERSION_MAJOR=0
    YAML_VERSION_MINOR=2
    YAML_VERSION_PATCH=5
    "YAML_VERSION_STRING=\"0.2.5.2\"")

# --- hev-task-system (mirrors third-part/hev-task-system/Android.mk) ---
file(GLOB_RECURSE TASK_SYSTEM_C_SRCS "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/src/*.c")
set(TASK_SYSTEM_ASM_SRCS "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/src/kern/task/hev-task-execute.S")
set_source_files_properties(${TASK_SYSTEM_ASM_SRCS} PROPERTIES LANGUAGE ASM)
add_library(hev-task-system STATIC ${TASK_SYSTEM_C_SRCS} ${TASK_SYSTEM_ASM_SRCS})
target_include_directories(hev-task-system PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/src"
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/src/arch/arm"
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/src/arch/x86"
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/hev-task-system/include")
target_compile_definitions(hev-task-system PUBLIC
    ENABLE_STACK_OVERFLOW_DETECTION
    ENABLE_MEMALLOC_SLICE
    ENABLE_IO_SPLICE_SYSCALL
    CONFIG_STACK_BACKEND=STACK_MMAP
    CONFIG_STACK_OVERFLOW_DETECTION=1
    CONFIG_MEMALLOC_SLICE_ALIGN=64
    CONFIG_MEMALLOC_SLICE_MAX_SIZE=4096
    CONFIG_MEMALLOC_SLICE_MAX_COUNT=1000
    CONFIG_SCHED_CLOCK=CLOCK_NONE)
target_compile_options(hev-task-system PRIVATE -fvisibility=hidden)

# --- lwip (mirrors third-part/lwip/Android.mk) ---
file(GLOB_RECURSE LWIP_SRCS "${CMAKE_CURRENT_SOURCE_DIR}/third-part/lwip/src/*.c")
add_library(lwip STATIC ${LWIP_SRCS})
target_include_directories(lwip PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/lwip/src/include"
    "${CMAKE_CURRENT_SOURCE_DIR}/third-part/lwip/src/ports/include")
target_compile_definitions(lwip PUBLIC FD_SET_DEFINED SOCKLEN_T_DEFINED)

# --- hev-socks5-tunnel (mirrors top-level Android.mk) ---
file(GLOB HEV_SRCS
    "${CMAKE_CURRENT_SOURCE_DIR}/src/*.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/misc/*.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/core/src/*.c")
list(REMOVE_ITEM HEV_SRCS "${CMAKE_CURRENT_SOURCE_DIR}/src/hev-jni.c")
add_library(hev-socks5-tunnel STATIC ${HEV_SRCS})
target_include_directories(hev-socks5-tunnel PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}/src"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/misc"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/core/include"
    "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_compile_definitions(hev-socks5-tunnel PRIVATE
    FD_SET_DEFINED
    SOCKLEN_T_DEFINED
    "COMMIT_ID=\"2.9.3\"")
target_link_libraries(hev-socks5-tunnel PUBLIC yaml lwip hev-task-system)
