pipeline {
    agent any
    options {
        timeout()
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
