pipeline {
    agent any
    stages {
        stage('P') {
            parallel {
                stage('A') { steps { echo '1' } }
                stage('A') { steps { echo '2' } }
            }
        }
    }
}
