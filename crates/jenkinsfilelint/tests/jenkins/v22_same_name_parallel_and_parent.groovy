pipeline {
    agent any
    stages {
        stage('P') {
            parallel {
                stage('P') { steps { echo 'x' } }
            }
        }
    }
}
