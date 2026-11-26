pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', passwordVariable: 'PASS')]) {
                    script {
                        def p = PASS
                        sh "login ${p}"
                    }
                }
            }
        }
    }
}
