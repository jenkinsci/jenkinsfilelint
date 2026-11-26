pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo """x ${
                    1 +
                    2
                } y"""
            }
        }
    }
}
