pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def x = 1 +
                    if (x) { echo 'a' }
                }
            }
        }
    }
}
