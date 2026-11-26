pipeline {
    agent any
    stages {
        stage('P') {
            parallel {
                failFast true
                stage('A') { steps { echo 'a' } }
                stage('B') { steps { echo 'b' } }
            }
        }
    }
}
