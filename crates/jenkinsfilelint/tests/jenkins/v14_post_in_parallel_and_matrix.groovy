pipeline {
    agent any
    stages {
        stage('P') {
            parallel {
                stage('A') {
                    steps { echo 'a' }
                    post { always { echo 'post a' } }
                }
            }
        }
    }
    post {
        always {
            script {
                if (currentBuild.result == 'FAILURE') {
                    echo 'failed'
                }
            }
        }
    }
}
