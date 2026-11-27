pipeline {
    agent any
    stages {
        stage('S') {
            when {
                allOf { }
            }
            steps { echo 'x' }
        }
    }
}
