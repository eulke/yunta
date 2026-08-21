# Copies the already-built release binary in — never recompiles.
# `release.yml`'s publish-container job assembles the build context
# as `ctx/<TARGETOS>/<TARGETARCH>/yunta` from the matching GitHub Release
# artifact before invoking `docker buildx build --platform
# linux/amd64,linux/arm64`, so each platform's build picks up its own
# already-compiled binary through Docker's automatic TARGETOS/TARGETARCH
# build args.
FROM alpine:3.20

RUN apk add --no-cache git ca-certificates

ARG TARGETOS
ARG TARGETARCH
COPY ctx/${TARGETOS}/${TARGETARCH}/yunta /usr/local/bin/yunta

ENTRYPOINT ["yunta"]
CMD ["--help"]
