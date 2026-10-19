pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    (A, B) = [1, 2]
                }
            }
        }
    }
}
