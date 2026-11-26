pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    outer:
                    for (x in [1, 2]) {
                        break outer
                    }
                }
            }
        }
    }
}
