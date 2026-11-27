pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    script {
                        def cmd = "deploy --token ${TOKEN}"
                        cmd = 'deploy --token "$TOKEN"'
                        sh cmd
                    }
                }
            }
        }
    }
}
