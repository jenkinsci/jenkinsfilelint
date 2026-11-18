pipeline {
    agent any
    stages {
        stage('Outer') {
            stages {
                stage('T') { steps { echo 'x' } }
                stage('T') { steps { echo 'y' } }
            }
        }
    }
}
