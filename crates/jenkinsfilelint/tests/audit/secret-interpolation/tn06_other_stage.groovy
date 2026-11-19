pipeline {
    agent any
    stages {
        stage('A') {
            environment {
                CREDS = credentials('nexus')
            }
            steps {
                sh 'curl -u "$CREDS_USR:$CREDS_PSW" https://example.com'
            }
        }
        stage('B') {
            steps {
                sh "echo ${CREDS_PSW}"
            }
        }
    }
}
