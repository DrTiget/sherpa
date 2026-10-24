# Сборочный образ: Ubuntu 22.04 (самый старый glibc из целевых — пакеты работают и на 24.04),
# Qt 6.8 LTS (системный Qt в 22.04 — 6.2, в 24.04 — 6.4; фронту нужен 6.5+ и QList::assign из 6.6).
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential cmake ninja-build pkg-config git curl ca-certificates file patchelf \
        python3 desktop-file-utils fakeroot dpkg-dev xz-utils \
        libgl1-mesa-dev libglu1-mesa-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev \
        libfreetype6-dev libdbus-1-dev libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 \
        libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libegl1 \
    && rm -rf /var/lib/apt/lists/*
# Qt скачивается build-linux.sh на хосте (из контейнера download.qt.io бывает недоступен) и монтируется в /opt/qt
ENV QT_ROOT=/opt/qt/current PATH=/opt/qt/current/bin:/root/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
RUN mkdir -p /opt/tools && cd /opt/tools \
    && for u in \
      https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage \
      https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage \
      https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage; do \
        curl -fsSLO "$u"; done \
    && chmod +x * && ln -s /opt/tools/linuxdeploy-x86_64.AppImage /opt/tools/linuxdeploy \
    && ln -s /opt/tools/linuxdeploy-plugin-qt-x86_64.AppImage /opt/tools/linuxdeploy-plugin-qt \
    && ln -s /opt/tools/linuxdeploy-plugin-appimage-x86_64.AppImage /opt/tools/linuxdeploy-plugin-appimage
ENV PATH=/opt/tools:$PATH APPIMAGE_EXTRACT_AND_RUN=1
