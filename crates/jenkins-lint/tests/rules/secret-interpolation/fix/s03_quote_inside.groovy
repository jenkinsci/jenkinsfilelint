pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 't', variable: 'TOKEN')]) {
                    sh "curl -H 'Authorization: ${TOKEN}' x"
                }
            }
        }
    }
}
