#!/bin/sh
# Интеграционный тест: поднимает linuxserver/openssh-server в Docker и гоняет exec по паролю и ключу.
# Тест собирается и запускается в контейнере servergui-dev (см. docker/run.sh) с --network host.
set -eu
cd "$(dirname "$0")/.."
command -v docker >/dev/null || { echo "Docker недоступен — интеграционный тест пропущен"; exit 0; }
T=$(mktemp -d); NAME=sherpa-sshd-$$; PORT=${SSH_TEST_PORT:-10122}
trap 'docker rm -f $NAME >/dev/null 2>&1; rm -rf "$T"' EXIT
docker/run.sh ssh-keygen -q -t ed25519 -N '' -f "$T/id" 2>/dev/null || ssh-keygen -q -t ed25519 -N '' -f "$T/id"
docker run -d --name $NAME -p $PORT:2222 -e PUID=1000 -e PGID=1000 -e USER_NAME=tester -e USER_PASSWORD=testpass123 \
  -e PASSWORD_ACCESS=true -e PUBLIC_KEY="$(cat "$T/id.pub")" linuxserver/openssh-server:latest >/dev/null
for i in $(seq 1 40); do
  docker logs $NAME 2>&1 | grep -q "done" && break; sleep 1
done
sleep 2
docker exec $NAME apk add --no-cache bash >/dev/null 2>&1 || echo "не удалось поставить bash в контейнер"
run_tests() { # $1 = ожидаемая оболочка улучшенного режима, остальное — аргументы cargo test
  expect=$1; shift
  DOCKER_ARGS="-v $T:/keys:ro -e SSH_TEST_HOST=127.0.0.1 -e SSH_TEST_PORT=$PORT -e SSH_TEST_KEY=/keys/id -e SSH_TEST_EXPECT=$expect" \
    docker/run.sh cargo test -p sherpa-core "$@" -- --nocapture --test-threads=1
}
echo "== сервер без zsh: fallback на bash =="
run_tests bash --test ssh_integration --test ssh_enhanced --test monitor_integration
echo "== сервер с zsh =="
docker exec $NAME apk add --no-cache zsh >/dev/null 2>&1 || { echo "не удалось поставить zsh в контейнер"; exit 1; }
run_tests zsh --test ssh_enhanced
