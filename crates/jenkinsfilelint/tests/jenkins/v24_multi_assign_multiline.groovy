pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    (A,
                        B,
                        C) = ['1', '2', '3']
                }
            }
        }
    }
}
