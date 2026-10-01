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

FROM build AS tests
RUN apt-get update && apt-get install -y --no-install-recommends ocl-icd-libopencl1 \
 && rm -rf /var/lib/apt/lists/*
COPY tests/ /src/tests/
COPY scripts/ /src/scripts/
CMD ["make", "test"]

FROM debian:trixie-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        libegl1 libgles2 libcjson1 ocl-icd-libopencl1 libstdc++6 ca-certificates \
        proxychains4 curl jq \
    && rm -rf /var/lib/apt/lists/* \
 && useradd -m -u 1000 miner
COPY --from=build /src/miner /usr/local/bin/miner
COPY scripts/container-entrypoint.sh /usr/local/bin/gbtc-entrypoint
RUN chmod 755 /usr/local/bin/gbtc-entrypoint
USER miner
WORKDIR /home/miner
ENTRYPOINT ["/usr/local/bin/gbtc-entrypoint"]
CMD ["/usr/local/bin/miner"]
