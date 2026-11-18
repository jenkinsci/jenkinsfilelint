pipeline {
    agent any
    environment {
        A = 'a' + B
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
