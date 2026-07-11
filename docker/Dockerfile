# jenkinsfilelint-server
#
# Minimal Jenkins image for local Declarative Pipeline validation.
# Only includes the plugins needed for the /pipeline-model-converter/validate endpoint.
# Runs in unsecured mode bound to 127.0.0.1 — safe for local-only use.
#
# Build:
#   docker build -t jenkinsfilelint-server docker/
#
# Run:
#   docker run -d --rm \
#     --name jenkinsfilelint-server \
#     -p 127.0.0.1:18080:8080 \
#     jenkinsfilelint-server

FROM jenkins/jenkins:lts-jdk21

LABEL org.opencontainers.image.source="https://github.com/jenkinsci/jenkinsfilelint"
LABEL org.opencontainers.image.description="Minimal Jenkins image for Declarative Pipeline syntax validation"
LABEL org.opencontainers.image.licenses="MIT"

USER root

# Install plugins — pipeline-model-definition provides the validate endpoint,
# configuration-as-code lets us bootstrap in unsecured mode without the setup wizard.
COPY plugins.txt /usr/share/jenkins/ref/plugins.txt
RUN jenkins-plugin-cli --plugin-file /usr/share/jenkins/ref/plugins.txt

# JCasC configuration: unsecured, no executors, loopback-only
COPY jenkins.yaml /usr/share/jenkins/ref/jenkins.yaml
ENV CASC_JENKINS_CONFIG=/usr/share/jenkins/ref/jenkins.yaml

USER jenkins

# Disable the setup wizard and update-center checks
ENV JAVA_OPTS="-Djenkins.install.runSetupWizard=false"

# Listen on all interfaces inside the container.
# Security is enforced by the Docker port mapping (-p 127.0.0.1:18080:8080)
# which binds to loopback on the host side.
ENV JENKINS_OPTS="--httpListenAddress=0.0.0.0"

EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -sf http://127.0.0.1:8080/login || exit 1
