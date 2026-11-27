pipeline {
    agent any
    stages {
        stage('S') {
            when {
            }
            steps { echo 'x' }
        }
    }
}
