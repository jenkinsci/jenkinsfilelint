pipeline {
    agent any
    environment {
        CREDS = credentials('nexus')
    }
    stages {
        stage('S') {
            steps {
                sh "curl -u ${CREDS_USR}:${CREDS_PSW} https://example.com"
                sh "use $CREDS"
            }
        }
    }
}
