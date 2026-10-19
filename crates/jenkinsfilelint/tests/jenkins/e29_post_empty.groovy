pipeline {
    agent any
    post {
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
