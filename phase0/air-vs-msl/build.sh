#!/bin/sh
# build.sh <out>: builds air-vs-msl.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
/usr/bin/clang++ -std=c++20 -O2 -fobjc-arc "$here/air-vs-msl.mm" -o "$1" -framework Metal -framework Foundation
