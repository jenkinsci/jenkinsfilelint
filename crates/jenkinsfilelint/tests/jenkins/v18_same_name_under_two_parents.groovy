pipeline {
    agent any
    stages {
        stage('Linux') {
            stages { stage('Test') { steps { echo 'l' } } }
        }
        stage('Windows') {
            stages { stage('Test') { steps { echo 'w' } } }
        }
    }
}
