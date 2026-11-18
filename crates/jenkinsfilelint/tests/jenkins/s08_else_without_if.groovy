pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    else { echo 'x' }
                }
            }
        }
    }
}
