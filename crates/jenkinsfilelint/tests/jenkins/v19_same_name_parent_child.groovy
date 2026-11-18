pipeline {
    agent any
    stages {
        stage('Build') {
            stages { stage('Build') { steps { echo 'b' } } }
        }
    }
}
