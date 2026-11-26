pipeline {
    agent any
    stages {
        stage('P') {
            parallel {
                stage('Linux') { stages { stage('Test') { steps { echo 'l' } } } }
                stage('Mac') { stages { stage('Test') { steps { echo 'm' } } } }
            }
        }
    }
}
