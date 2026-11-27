pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 't', variable: 'TOKEN')]) {
                    pwsh "Deploy -Token $TOKEN"
                }
            }
        }
    }
}
