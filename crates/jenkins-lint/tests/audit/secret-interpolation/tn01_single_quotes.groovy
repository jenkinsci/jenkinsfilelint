pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([usernamePassword(credentialsId: 'nexus', usernameVariable: 'USER', passwordVariable: 'PASS')]) {
                    sh 'curl -u "$USER:$PASS" https://nexus.example.com'
                }
            }
        }
    }
}
