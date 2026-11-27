pipeline {
    agent any
    stages {
        stage('S') {
            parallel {
                stage('A') {
                    parallel {
                        stage('A1') { steps { echo 'x' } }
                    }
                }
            }
        }
    }
}
