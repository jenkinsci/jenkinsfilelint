pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def r = $/abc
                }
            }
        }
    }
}
