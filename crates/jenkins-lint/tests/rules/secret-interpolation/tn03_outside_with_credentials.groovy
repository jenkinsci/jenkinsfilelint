pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "echo ${TOKEN}"
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    sh 'deploy --token "$TOKEN"'
                }
                sh "echo ${TOKEN}"
            }
        }
    }
}
