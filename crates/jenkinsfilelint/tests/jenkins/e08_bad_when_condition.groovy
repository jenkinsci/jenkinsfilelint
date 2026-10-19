pipeline {
    agent any
    stages {
        stage('S') {
            when { brnch 'main' }
            steps { echo 'x' }
        }
    }
}
