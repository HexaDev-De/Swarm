# The donor image ships the helper @cloudflare/sandbox uses; keep its tag equal to the
# installed @cloudflare/sandbox version.
ARG SANDBOX_TOOLS_IMAGE=docker.io/cloudflare/sandbox:1.0.1
FROM ${SANDBOX_TOOLS_IMAGE} AS sandbox-tools

# A small Debian with Node, Python and Git, so agents can build and run most projects.
FROM node:22-bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends bash ca-certificates curl git python3 python3-pip unzip \
  && rm -rf /var/lib/apt/lists/*
COPY --from=sandbox-tools /usr/local/bin/sandbox-shim /usr/local/bin/sandbox-shim
RUN mkdir -p /workspace
CMD ["sleep", "infinity"]
