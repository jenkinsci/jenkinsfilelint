pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 't', variable: 'TOKEN')]) {
                    bat "deploy %X% ${TOKEN}"
                }
            }
        }
    }
}
