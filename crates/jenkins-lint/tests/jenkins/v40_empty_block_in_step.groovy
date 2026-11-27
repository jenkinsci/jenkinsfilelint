pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                echo 'Hello'
                dir('build') {
                }
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    // TODO: publish
                }
            }
            post {
                cleanup {
                    node(null) {
                    }
                }
            }
        }
    }
}
