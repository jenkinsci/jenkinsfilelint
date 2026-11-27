pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 't', variable: 'TOKEN')]) {
                    echo "token ${TOKEN}"
                }
            }
        }
    }
}
