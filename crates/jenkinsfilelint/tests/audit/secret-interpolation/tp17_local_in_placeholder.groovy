pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([usernamePassword(credentialsId: 'nexus', usernameVariable: 'USER', passwordVariable: 'PASS')]) {
                    script {
                        def auth = "${USER}:${PASS}"
                        sh "curl -u ${auth} https://nexus.example.com"
                    }
                }
            }
        }
    }
}
