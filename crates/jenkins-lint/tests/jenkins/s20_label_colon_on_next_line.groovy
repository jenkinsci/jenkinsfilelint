pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def n = 0
                    outer
                    : for (x in [1, 2]) {
                        break outer
                    }
                }
            }
        }
    }
}
