FROM debian:trixie-slim AS build
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config ca-certificates \
        libegl-dev libgles-dev libgl-dev libcjson-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Makefile /src/
COPY src/ /src/src/
RUN make \
 && strip miner

FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        libegl1 libegl-mesa0 libgles2 libgl1-mesa-dri \
        libcjson1 libdrm2 libdrm-intel1 libgbm1 \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd -g 992 render \
 && useradd -m -u 1000 miner \
 && usermod -aG video,render miner
COPY --from=build /src/miner /usr/local/bin/miner
USER miner
WORKDIR /home/miner
ENTRYPOINT ["/usr/local/bin/miner"]
