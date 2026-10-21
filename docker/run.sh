#!/bin/sh
# Запуск команды в среде сборки: docker/run.sh cargo test -p servergui-core
cd "$(dirname "$0")/.." || exit 1
exec docker run --rm --network host -u "$(id -u):$(id -g)" -e HOME=/tmp/h -e CARGO_HOME=/cargo \
  -v servergui-cargo:/cargo -v "$PWD":/work -w /work ${DOCKER_ARGS} servergui-dev "$@"
