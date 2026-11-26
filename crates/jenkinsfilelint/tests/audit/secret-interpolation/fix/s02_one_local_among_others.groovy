pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', passwordVariable: 'PASS')]) {
                    script {
                        def host = "example.com"
                        sh "curl -u ${USER}:${PASS} ${host}"
                    }
                }
            }
        }
    }
}
