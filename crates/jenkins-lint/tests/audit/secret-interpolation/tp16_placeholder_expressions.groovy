pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    sh "deploy --token ${TOKEN.trim()}"
                    sh "deploy --token ${URLEncoder.encode(TOKEN, 'UTF-8')}"
                }
            }
        }
    }
}
