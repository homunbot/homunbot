FROM node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        curl \
        git \
        python3 \
        python3-venv \
        tini \
    && rm -rf /var/lib/apt/lists/*

# Core runtime contract for sandboxed skills:
# - node / npm / npx (from base image)
# - bash
# - python3
# - tsx for TypeScript skill scripts
RUN npm install -g tsx

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["bash"]
