pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def (x, y) = [1, 2]
                }
            }
        }
    }
}
