#!/bin/sh
set -eu
fan_kernel="$1"
fan_arch="$2"
fan_symbols=$(find /var/lib/dkms/tuxedo-drivers -path "*/$fan_kernel/$fan_arch/module/Module.symvers" -type f | sort -V | tail -n 1)
[ -n "$fan_symbols" ] || { echo 'TUXEDO sürücüsünün bu kernel için sembolleri yok.' >&2; exit 1; }
fan_build="/lib/modules/$fan_kernel/build"
[ -f "$fan_build/Makefile" ] || { echo 'Kernel geliştirme başlıkları veya Makefile bulunamadı.' >&2; exit 1; }
fan_config="$fan_build/.config"
if [ ! -f "$fan_config" ] || [ ! -r "$fan_config" ]; then
    fan_config="$fan_build/include/config/auto.conf"
fi
[ -f "$fan_config" ] && [ -r "$fan_config" ] || { echo 'Kernel derleyici yapılandırması okunamadı (.config veya auto.conf).' >&2; exit 1; }
fan_compiler=''
if grep -q '^CONFIG_CC_IS_CLANG=y$' "$fan_config"; then
    fan_compiler='LLVM=1'
else
    fan_grep_status=$?
    [ "$fan_grep_status" -eq 1 ] || { echo 'Kernel derleyici yapılandırması sorgulanamadı.' >&2; exit 1; }
fi
exec make -C "$fan_build" M="$PWD" $fan_compiler KBUILD_EXTRA_SYMBOLS="$fan_symbols" modules
