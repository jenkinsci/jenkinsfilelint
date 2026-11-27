# Built by release.yml from the musl binaries in dist/<platform>/.
# Alpine, not scratch: CI agents need sh and cat, --remote needs CA certs.
# No ENTRYPOINT, so CI systems can run their own command.
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
