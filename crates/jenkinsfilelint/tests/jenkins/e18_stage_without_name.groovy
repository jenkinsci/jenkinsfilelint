pipeline {
    agent any
    stages {
        stage {
            steps { echo 'x' }
        }
    }
}
