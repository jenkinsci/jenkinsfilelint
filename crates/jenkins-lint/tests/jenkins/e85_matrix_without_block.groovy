pipeline {
    agent any
    stages {
        stage('M') {
            matrix 'x'
        }
    }
}
