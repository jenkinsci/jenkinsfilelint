pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', passwordVariable: 'PASS')]) {
                    sh "curl -u ${USER}:${PASS} https://example.com"
                }
            }
        }
    }
}
