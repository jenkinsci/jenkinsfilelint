pipeline {
    agent any
    stages {
        stage('S') {
            stages {
                stage('A') { steps { echo 'a' } }
            }
            parallel {
                stage('B') { steps { echo 'b' } }
            }
        }
    }
}
