pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def (String x, int y) = ['a', 1]
                }
            }
        }
    }
}
