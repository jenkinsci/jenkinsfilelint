pipeline {
    agent any
    stages {
        stage('S') {
            environment {
                CREDS = credentials('nexus')
            }
            steps {
                sh "curl -u ${CREDS_USR}:${CREDS_PSW} https://example.com"
            }
        }
    }
}
