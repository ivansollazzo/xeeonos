#!/bin/bash
# Recipe step (xapp build, inside the container): the checks of the test
# application, and a file only its own image has.
set -euo pipefail
install -Dm0755 /recipe/hello-check "$ROOT/usr/bin/hello-check"
install -Dm0644 /dev/stdin "$ROOT/usr/share/xapp-hello/marker" <<< "image of org.xeeonos.Hello"
