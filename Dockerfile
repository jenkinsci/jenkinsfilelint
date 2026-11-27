# The jenkins-lint image: the static binary of the release, on Alpine.
#
# Alpine rather than scratch, because CI systems run their steps in the
# container with a shell: a Jenkins docker agent keeps it running with `cat`
# and runs each `sh` step with /bin/sh, and so do GitLab CI and Kubernetes
# pod templates. Alpine also has the CA certificates that --remote needs.
#
# There is no ENTRYPOINT, so that those CI systems can run their own
# command: `docker run --rm -v "$PWD:/src" IMAGE jenkins-lint .`
#
# The binaries are not built here: the docker job of release.yml unpacks
# the musl archives of the wheels job, the binaries of the tested wheels,
# into dist/<platform>/jenkins-lint.
FROM alpine:3.24@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf

ARG TARGETPLATFORM
ARG VERSION=dev

LABEL org.opencontainers.image.title="jenkins-lint" \
      org.opencontainers.image.description="Check Jenkinsfiles offline: Groovy syntax, Declarative structure and security" \
      org.opencontainers.image.source="https://github.com/jenkinsci/jenkins-lint" \
      org.opencontainers.image.documentation="https://jenkinsci.github.io/jenkins-lint/" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}"

COPY --chmod=755 dist/${TARGETPLATFORM}/jenkins-lint /usr/local/bin/jenkins-lint

WORKDIR /src
CMD ["jenkins-lint", "--help"]
