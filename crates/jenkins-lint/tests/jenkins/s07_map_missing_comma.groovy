pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def m = [a: 1 b: 2]
                }
            }
        }
    }
}
