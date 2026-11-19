pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    bat "deploy.cmd ${TOKEN}"
                    powershell "Deploy -Token ${TOKEN}"
                    pwsh "Deploy -Token $TOKEN"
                }
            }
        }
    }
}
