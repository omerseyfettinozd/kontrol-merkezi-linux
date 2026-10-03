#!/bin/sh
set -eu
fan_kernel="$1"
fan_arch="$2"
fan_symbols=$(find /var/lib/dkms/tuxedo-drivers -path "*/$fan_kernel/$fan_arch/module/Module.symvers" -type f | sort -V | tail -n 1)
[ -n "$fan_symbols" ] || { echo 'TUXEDO sürücüsünün bu kernel için sembolleri yok.' >&2; exit 1; }
fan_compiler=''
if rg -q '^CONFIG_CC_IS_CLANG=y' "/lib/modules/$fan_kernel/build/.config"; then fan_compiler='LLVM=1'; fi
exec make -C "/lib/modules/$fan_kernel/build" M="$PWD" $fan_compiler KBUILD_EXTRA_SYMBOLS="$fan_symbols" modules
