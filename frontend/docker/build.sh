#!/bin/sh
# Сборка и тесты в контейнере sherpa-qt (см. docker/Dockerfile). Запуск из frontend/:
#   docker build -t sherpa-qt docker && docker run --rm -v $PWD:/src -v $PWD/../core/target/release:/core:ro sherpa-qt /src/docker/build.sh
set -e
cmake -S /src -B /tmp/build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/build
cd /tmp/build
export SHERPA_CORE=/core/sherpa-core QT_QPA_PLATFORM=offscreen SHERPA_SHOTS_DIR=/src/screenshots
ctest --output-on-failure
