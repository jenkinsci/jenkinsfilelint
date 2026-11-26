pipeline {
    agent any
    stages {
        stage('S') {
            steps { echo 'x' }
            parallel {
                stage('A') { steps { echo 'a' } }
            }
        }
    }
}
