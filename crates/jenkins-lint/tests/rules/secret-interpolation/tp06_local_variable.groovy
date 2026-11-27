pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    script {
                        def cmd = "curl -H 'Authorization: Bearer ${TOKEN}' https://api.example.com"
                        sh cmd
                        def out = sh(script: cmd, returnStdout: true)
                    }
                }
            }
        }
    }
}
