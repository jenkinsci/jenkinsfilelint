pipeline {
    agent any
    stages {
        stage('P') {
            failFast true
            parallel {
                stage('A') { steps { echo 'a' } }
                stage('B') { steps { echo 'b' } }
            }
        }
    }
}
