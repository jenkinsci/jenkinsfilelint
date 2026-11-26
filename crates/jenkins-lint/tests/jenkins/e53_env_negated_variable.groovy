pipeline {
    agent any
    environment {
        A = -B
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
